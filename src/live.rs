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

/// Minimum character count for OCR text to be worth translating.
const MIN_TEXT_LEN: usize = 5;

/// Similarity threshold (0.0-1.0). If two texts are this similar, skip re-translation.
const SIMILARITY_THRESHOLD: f64 = 0.80;

/// Truncate a string to at most `max` characters, respecting char boundaries.
fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Normalize text for comparison: collapse whitespace, lowercase, trim.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Compute similarity ratio between two strings (0.0 = completely different, 1.0 = identical).
/// Uses longest common subsequence ratio — fast enough for short subtitle strings.
fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // Quick length-based rejection
    let max_len = m.max(n);
    let min_len = m.min(n);
    if (min_len as f64 / max_len as f64) < SIMILARITY_THRESHOLD {
        return min_len as f64 / max_len as f64;
    }

    // LCS using two rows
    let mut prev = vec![0u16; n + 1];
    let mut curr = vec![0u16; n + 1];
    for i in 1..=m {
        for j in 1..=n {
            curr[j] = if a_chars[i - 1] == b_chars[j - 1] {
                prev[j - 1] + 1
            } else {
                prev[j].max(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.iter_mut().for_each(|x| *x = 0);
    }
    let lcs = prev[n] as f64;
    (2.0 * lcs) / (m + n) as f64
}

pub async fn start_live_monitor(
    region: Region,
    ocr_lang: &str,
    source_lang: &str,
    target_lang: &str,
    backend: &dyn TranslationBackendDyn,
    interval_ms: u64,
    settle_time_ms: u64,
    stop_signal: &AtomicBool,
    translated_text: &Arc<Mutex<String>>,
) -> Result<()> {
    let settle_time = settle_time_ms as u128;
    let mut prev_image: Option<DynamicImage> = None;
    let diff_threshold: u8 = 10;
    let change_ratio: f64 = 0.01;

    // Text settling state
    let mut current_ocr_normalized: Option<String> = None;
    let mut current_ocr_raw: Option<String> = None;
    let mut text_last_changed: Instant = Instant::now();
    let mut text_settled: bool = false;
    // Track the last few translated texts for fuzzy dedup
    let mut translated_history: Vec<String> = Vec::new();

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
                if let Some(ref norm) = current_ocr_normalized {
                    let elapsed = text_last_changed.elapsed().as_millis();
                    if elapsed >= settle_time {
                        if !is_duplicate(norm, &translated_history) {
                            let raw = current_ocr_raw.as_deref().unwrap_or(norm);
                            log::info!("[tick {}] Text settled after {}ms, translating: {}",
                                tick_count, elapsed, truncate_chars(raw, 120));
                            do_translate(backend, raw, source_lang, target_lang, translated_text, &mut translated_history).await;
                        } else {
                            log::debug!("[tick {}] Skipping duplicate text", tick_count);
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
                current_ocr_normalized = None;
                current_ocr_raw = None;
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

        // Skip garbage / too-short OCR
        let normalized = normalize(&text);
        if normalized.len() < MIN_TEXT_LEN {
            continue;
        }

        log::info!("[tick {}] OCR: {}", tick_count, truncate_chars(&text, 120));

        // Compare normalized text to detect changes (handles OCR jitter like "aman" vs "a man")
        let text_changed = match &current_ocr_normalized {
            Some(prev) => similarity(prev, &normalized) < 0.95,
            None => true,
        };

        if text_changed {
            current_ocr_normalized = Some(normalized);
            current_ocr_raw = Some(text);
            text_last_changed = Instant::now();
            text_settled = false;
        } else {
            // Same text — check if settled
            let elapsed = text_last_changed.elapsed().as_millis();
            if !text_settled && elapsed >= settle_time {
                let norm = current_ocr_normalized.as_ref().unwrap();
                if !is_duplicate(norm, &translated_history) {
                    let raw = current_ocr_raw.as_deref().unwrap_or(norm);
                    log::info!("[tick {}] Text settled after {}ms, translating: {}",
                        tick_count, elapsed, truncate_chars(raw, 120));
                    do_translate(backend, raw, source_lang, target_lang, translated_text, &mut translated_history).await;
                } else {
                    log::debug!("[tick {}] Skipping duplicate text", tick_count);
                }
                text_settled = true;
            }
        }
    }

    Ok(())
}

/// Check if normalized text is too similar to any recently translated text.
fn is_duplicate(normalized: &str, history: &[String]) -> bool {
    history.iter().any(|prev| similarity(prev, normalized) >= SIMILARITY_THRESHOLD)
}

async fn do_translate(
    backend: &dyn TranslationBackendDyn,
    text: &str,
    source_lang: &str,
    target_lang: &str,
    translated_text: &Arc<Mutex<String>>,
    history: &mut Vec<String>,
) {
    let start = Instant::now();
    match backend.translate_dyn(text.to_owned(), source_lang.to_owned(), target_lang.to_owned()).await {
        Ok(translated) => {
            let ms = start.elapsed().as_millis();
            log::info!("Translation ({}ms): {}", ms, truncate_chars(&translated, 120));
            *translated_text.lock().unwrap() = translated;

            // Keep last 10 translated source texts for dedup
            let norm = normalize(text);
            history.push(norm);
            if history.len() > 10 {
                history.remove(0);
            }
        }
        Err(e) => {
            log::warn!("Translation failed: {}", e);
            *translated_text.lock().unwrap() = format!("[Translation error: {}]", e);
        }
    }
}
