use anyhow::Result;
use image::DynamicImage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::time::{self, Duration};

use crate::capture;
use crate::diff;
#[cfg(feature = "ocr")]
use crate::ocr;
use crate::selector::Region;
use crate::translate::TranslationBackendDyn;

/// How long (ms) the OCR text must remain unchanged before we consider it "complete".
/// This handles text that animates in character-by-character (subtitles, typewriter effects).
const SETTLE_TIME_MS: u128 = 1500;

/// Truncate a string to at most `max` characters, respecting char boundaries.
fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

pub async fn start_live_monitor(
    region: Region,
    ocr_lang: &str,
    source_lang: &str,
    target_lang: &str,
    backend: &dyn TranslationBackendDyn,
    interval_ms: u64,
    stop_signal: &AtomicBool,
    translated_text: &Arc<Mutex<String>>,
) -> Result<()> {
    let mut prev_image: Option<DynamicImage> = None;
    let diff_threshold: u8 = 10;
    let change_ratio: f64 = 0.01;

    // Text settling state: track when the OCR text last changed
    let mut current_ocr_text: Option<String> = None;
    let mut text_last_changed: Instant = Instant::now();
    let mut text_settled: bool = false; // has this text already been sent for translation?
    let mut last_translated_text: Option<String> = None;

    let mut tick_count: u64 = 0;
    let mut interval = time::interval(Duration::from_millis(interval_ms));

    loop {
        interval.tick().await;
        tick_count += 1;

        if stop_signal.load(Ordering::SeqCst) {
            log::info!("Live mode stopped by user");
            break;
        }

        // Capture the region
        let current = match capture::capture_region(
            region.x,
            region.y,
            region.width,
            region.height,
        ) {
            Ok(img) => img,
            Err(e) => {
                log::warn!("[tick {}] Live capture failed: {}", tick_count, e);
                continue;
            }
        };

        // Check if image changed
        let changed = match &prev_image {
            Some(prev) => diff::images_differ(prev, &current, diff_threshold, change_ratio),
            None => true,
        };

        if !changed {
            // Image didn't change — but check if we have settled text waiting to translate
            if !text_settled {
                if let Some(ref text) = current_ocr_text {
                    let elapsed = text_last_changed.elapsed().as_millis();
                    if elapsed >= SETTLE_TIME_MS {
                        // Text has been stable long enough — translate it
                        if last_translated_text.as_deref() != Some(text) {
                            log::info!("[tick {}] Text settled after {}ms, translating: {}",
                                tick_count, elapsed, truncate_chars(text, 120));
                            do_translate(backend, text, source_lang, target_lang, translated_text, &mut last_translated_text).await;
                        }
                        text_settled = true;
                    }
                }
            }
            continue;
        }

        prev_image = Some(current.clone());

        // OCR
        #[cfg(feature = "ocr")]
        let text = match ocr::extract_text(&current, ocr_lang) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => {
                log::debug!("[tick {}] OCR: (empty)", tick_count);
                current_ocr_text = None;
                text_settled = false;
                continue;
            }
            Err(e) => {
                log::warn!("[tick {}] Live OCR failed: {}", tick_count, e);
                continue;
            }
        };
        #[cfg(not(feature = "ocr"))]
        {
            let _ = ocr_lang;
            log::error!("OCR feature not enabled");
            break;
        }

        // Log every OCR capture so the user can see what's being read
        log::info!("[tick {}] OCR: {}", tick_count, truncate_chars(&text, 120));

        // Check if text changed from last OCR read
        let text_changed = current_ocr_text.as_deref() != Some(&text);

        if text_changed {
            // Text is still changing — reset the settle timer
            current_ocr_text = Some(text);
            text_last_changed = Instant::now();
            text_settled = false;
        } else {
            // Same text as before — check if it's been stable long enough
            let elapsed = text_last_changed.elapsed().as_millis();
            if !text_settled && elapsed >= SETTLE_TIME_MS {
                let text = current_ocr_text.as_ref().unwrap();
                if last_translated_text.as_deref() != Some(text) {
                    log::info!("[tick {}] Text settled after {}ms, translating: {}",
                        tick_count, elapsed, truncate_chars(text, 120));
                    do_translate(backend, text, source_lang, target_lang, translated_text, &mut last_translated_text).await;
                }
                text_settled = true;
            }
        }
    }

    Ok(())
}

async fn do_translate(
    backend: &dyn TranslationBackendDyn,
    text: &str,
    source_lang: &str,
    target_lang: &str,
    translated_text: &Arc<Mutex<String>>,
    last_translated_text: &mut Option<String>,
) {
    let start = Instant::now();
    match backend.translate_dyn(text.to_owned(), source_lang.to_owned(), target_lang.to_owned()).await {
        Ok(translated) => {
            let ms = start.elapsed().as_millis();
            log::info!("Translation ({}ms): {}", ms, truncate_chars(&translated, 120));
            *translated_text.lock().unwrap() = translated;
            *last_translated_text = Some(text.to_owned());
        }
        Err(e) => {
            log::warn!("Translation failed: {}", e);
            *translated_text.lock().unwrap() = format!("[Translation error: {}]", e);
        }
    }
}
