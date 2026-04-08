use anyhow::Result;
use image::DynamicImage;
use tokio::time::{self, Duration};

use crate::capture;
use crate::diff;
#[cfg(feature = "ocr")]
use crate::ocr;
use crate::overlay::LiveOverlayState;
use crate::selector::Region;
use crate::translate::TranslationBackendDyn;

pub async fn start_live_monitor(
    region: Region,
    ocr_lang: &str,
    source_lang: &str,
    target_lang: &str,
    backend: &dyn TranslationBackendDyn,
    interval_ms: u64,
    overlay_state: &LiveOverlayState,
) -> Result<()> {
    let mut prev_image: Option<DynamicImage> = None;
    let diff_threshold: u8 = 10;
    let change_ratio: f64 = 0.01; // 1% of pixels must change

    let mut interval = time::interval(Duration::from_millis(interval_ms));

    loop {
        interval.tick().await;

        // Check if overlay was closed
        if *overlay_state.should_close.lock().unwrap() {
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
            None => true, // First capture always processes
        };

        if !changed {
            continue;
        }

        log::info!("Screen content changed, re-processing...");
        prev_image = Some(current.clone());

        // OCR
        let text = match ocr::extract_text(&current, ocr_lang) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => continue,
            Err(e) => {
                log::warn!("Live OCR failed: {}", e);
                continue;
            }
        };

        // Update source text in overlay
        *overlay_state.source_text.lock().unwrap() = text.clone();

        // Translate
        match backend.translate_dyn(text.clone(), source_lang.to_owned(), target_lang.to_owned()).await {
            Ok(translated) => {
                *overlay_state.translated_text.lock().unwrap() = translated;
            }
            Err(e) => {
                log::warn!("Live translation failed: {}", e);
                *overlay_state.translated_text.lock().unwrap() =
                    format!("[Translation error: {}]", e);
            }
        }
    }

    Ok(())
}
