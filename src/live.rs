use anyhow::Result;
use image::DynamicImage;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{self, Duration};

use crate::capture;
use crate::diff;
#[cfg(feature = "ocr")]
use crate::ocr;
use crate::selector::Region;
use crate::translate::TranslationBackendDyn;

/// Number of consecutive stable OCR reads before translating.
const STABLE_READS_REQUIRED: u32 = 2;

pub async fn start_live_monitor(
    region: Region,
    ocr_lang: &str,
    source_lang: &str,
    target_lang: &str,
    backend: &dyn TranslationBackendDyn,
    interval_ms: u64,
    stop_signal: &AtomicBool,
) -> Result<()> {
    let mut prev_image: Option<DynamicImage> = None;
    let diff_threshold: u8 = 10;
    let change_ratio: f64 = 0.01;

    let mut last_ocr_text: Option<String> = None;
    let mut stable_count: u32 = 0;
    let mut last_translated_text: Option<String> = None;

    let mut interval = time::interval(Duration::from_millis(interval_ms));

    loop {
        interval.tick().await;

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
                log::warn!("Live capture failed: {}", e);
                continue;
            }
        };

        // Check if image changed
        let changed = match &prev_image {
            Some(prev) => diff::images_differ(prev, &current, diff_threshold, change_ratio),
            None => true,
        };

        if !changed {
            continue;
        }

        prev_image = Some(current.clone());

        // OCR
        #[cfg(feature = "ocr")]
        let text = match ocr::extract_text(&current, ocr_lang) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => {
                last_ocr_text = None;
                stable_count = 0;
                continue;
            }
            Err(e) => {
                log::warn!("Live OCR failed: {}", e);
                continue;
            }
        };
        #[cfg(not(feature = "ocr"))]
        {
            let _ = ocr_lang;
            log::error!("OCR feature not enabled");
            break;
        }

        // Debounce: check if OCR text is the same as last time
        if last_ocr_text.as_deref() == Some(&text) {
            stable_count += 1;
        } else {
            log::debug!("OCR text changed, waiting for stabilization...");
            last_ocr_text = Some(text.clone());
            stable_count = 1;
        }

        if stable_count < STABLE_READS_REQUIRED {
            continue;
        }

        // Don't re-translate identical text
        if last_translated_text.as_deref() == Some(&text) {
            continue;
        }

        log::info!("Text stable, translating: {}", &text[..text.len().min(60)]);

        match backend.translate_dyn(text.clone(), source_lang.to_owned(), target_lang.to_owned()).await {
            Ok(translated) => {
                log::info!("Live translation: {}", &translated[..translated.len().min(80)]);
                notify_translation(&translated);
                last_translated_text = Some(text);
            }
            Err(e) => {
                log::warn!("Live translation failed: {}", e);
            }
        }
    }

    Ok(())
}

/// Show a desktop notification with the translated text (replaces previous).
fn notify_translation(text: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg("-a").arg("Eyeclipse")
        .arg("-u").arg("normal")
        .arg("-h").arg("string:x-canonical-private-synchronous:eyeclipse-live")
        .arg("Translation")
        .arg(text)
        .spawn();
}
