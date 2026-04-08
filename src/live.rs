use anyhow::Result;
use image::DynamicImage;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::time::{self, Duration};

use crate::capture;
use crate::config::LiveTimingMode;
use crate::diff;
#[cfg(feature = "ocr")]
use crate::ocr;
use crate::selector::Region;
use crate::translate::TranslationBackendDyn;

/// Per-language tuning profile for OCR settle logic.
/// CJK and Latin scripts have different characteristics:
/// - CJK subtitles tend to stay on screen longer and OCR is noisier
/// - Latin subtitles change faster but OCR is more confident
pub struct LanguageProfile {
    /// How many consecutive OCR readings must agree before we consider text stable.
    pub required_agreements: usize,
    /// Minimum OCR confidence to accept a reading.
    pub min_confidence: i32,
    /// Minimum time (ms) text must be stable before translating.
    pub min_settle_ms: u128,
    /// How similar consecutive OCR readings must be to count as "agreeing" (0.0-1.0).
    pub consecutive_agreement_threshold: f64,
    /// If two texts are this similar, skip re-translation (0.0-1.0).
    pub similarity_threshold: f64,
    /// Minimum character count for OCR text to be worth translating.
    pub min_text_len: usize,
}

impl LanguageProfile {
    /// Profile for CJK languages (Japanese, Chinese, Korean).
    /// More agreements required because OCR is noisier.
    pub fn cjk() -> Self {
        Self {
            required_agreements: 3,
            min_confidence: 30,
            min_settle_ms: 500,
            consecutive_agreement_threshold: 0.90,
            similarity_threshold: 0.80,
            min_text_len: 5,
        }
    }

    /// Profile for Latin-script languages (English, French, Vietnamese, etc.).
    /// Fewer agreements needed — subtitles change faster and OCR is more reliable.
    pub fn latin() -> Self {
        Self {
            required_agreements: 2,
            min_confidence: 50,
            min_settle_ms: 400,
            consecutive_agreement_threshold: 0.90,
            similarity_threshold: 0.80,
            min_text_len: 5,
        }
    }

    /// Auto-detect the right profile from the OCR language string.
    #[cfg(feature = "ocr")]
    pub fn detect(ocr_lang: &str) -> Self {
        if ocr::is_cjk_lang(ocr_lang) {
            Self::cjk()
        } else {
            Self::latin()
        }
    }
}

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

/// Extract the longest substantive line for change detection.
/// Short OCR noise lines (furigana, artifacts) are ignored so they
/// don't disrupt settle-time tracking.
fn extract_primary_line(text: &str, min_text_len: usize) -> String {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| l.chars().count() >= min_text_len)
        .max_by_key(|l| l.chars().count())
        .unwrap_or("")
        .to_string()
}

/// Compute similarity ratio between two strings (0.0 = completely different, 1.0 = identical).
/// Uses longest common subsequence ratio — fast enough for short subtitle strings.
fn similarity(a: &str, b: &str, threshold: f64) -> f64 {
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
    if (min_len as f64 / max_len as f64) < threshold {
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
    timing_mode: &LiveTimingMode,
    stop_signal: &AtomicBool,
    translated_text: &Arc<Mutex<String>>,
) -> Result<()> {
    let mut prev_image: Option<DynamicImage> = None;
    let diff_threshold: u8 = 10;
    let change_ratio: f64 = 0.01;

    #[cfg(feature = "ocr")]
    let profile = LanguageProfile::detect(ocr_lang);
    #[cfg(not(feature = "ocr"))]
    let profile = LanguageProfile::latin();

    // Recent OCR readings for consecutive agreement check (primary line only, for change detection)
    let mut recent_readings: VecDeque<String> = VecDeque::new();
    // The full OCR text corresponding to the latest stable reading (sent to translation)
    let mut full_text_for_translation: String = String::new();
    // When the current "group" of similar readings started
    let mut group_start: Instant = Instant::now();
    // The last text that was actually sent for translation (normalized)
    let mut last_translated_normalized: Option<String> = None;
    // Already translated this stable group?
    let mut group_translated: bool = false;
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
            // Image unchanged — check if we should translate the stable group
            if !group_translated && recent_readings.len() >= profile.required_agreements {
                let elapsed = group_start.elapsed().as_millis();
                if elapsed >= profile.min_settle_ms {
                    // All recent readings agree and enough time has passed
                    let to_translate = if full_text_for_translation.is_empty() {
                        recent_readings.back().unwrap().clone()
                    } else {
                        full_text_for_translation.clone()
                    };
                    if !is_duplicate(&to_translate, &translated_history, profile.similarity_threshold) {
                        log::info!("[tick {}] Text stable ({} agreements, {}ms), translating: {}",
                            tick_count, recent_readings.len(), elapsed,
                            truncate_chars(&to_translate, 120));
                        do_translate(backend, &to_translate, source_lang, target_lang, translated_text, &mut translated_history).await;
                        last_translated_normalized = Some(normalize(&to_translate));
                    } else {
                        log::debug!("[tick {}] Skipping duplicate stable text", tick_count);
                    }
                    group_translated = true;
                }
            }
            continue;
        }

        prev_image = Some(current.clone());

        // OCR
        #[cfg(feature = "ocr")]
        let ocr_result = match ocr::extract_text(&current, ocr_lang) {
            Ok(r) if !r.text.is_empty() => r,
            Ok(_) => {
                recent_readings.clear();
                group_translated = false;
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

        #[cfg(feature = "ocr")]
        {
            // Confidence check
            if ocr_result.confidence < profile.min_confidence {
                log::debug!("[tick {}] Low confidence ({}), skipping", tick_count, ocr_result.confidence);
                continue;
            }

            // Garbage check
            if ocr::is_garbage_text(&ocr_result.text) {
                log::debug!("[tick {}] Garbage text detected, skipping", tick_count);
                continue;
            }

            let text = &ocr_result.text;

            // Extract primary line for change detection (longest substantive line)
            let primary = extract_primary_line(text, profile.min_text_len);
            if primary.chars().count() < profile.min_text_len {
                continue;
            }

            // Strip leading noise for comparison
            let cleaned = ocr::strip_leading_noise(&primary);
            if cleaned.chars().count() < profile.min_text_len {
                continue;
            }

            log::info!("[tick {}] OCR (conf={}): {}", tick_count,
                ocr_result.confidence, truncate_chars(text, 120));

            let normalized = normalize(cleaned);

            // --- Instant mode: translate on first good reading that differs ---
            if *timing_mode == LiveTimingMode::Instant {
                let dominated = last_translated_normalized.as_ref()
                    .map(|last| similarity(last, &normalized, profile.similarity_threshold) >= profile.similarity_threshold)
                    .unwrap_or(false);

                if !dominated && !is_duplicate(text, &translated_history, profile.similarity_threshold) {
                    log::info!("[tick {}] Instant mode — translating: {}",
                        tick_count, truncate_chars(text, 120));
                    do_translate(backend, text, source_lang, target_lang, translated_text, &mut translated_history).await;
                    last_translated_normalized = Some(normalize(text));
                }
                continue;
            }

            // --- Settle mode: wait for consecutive agreements + settle time ---

            // Check if this reading agrees with recent readings
            let agrees_with_recent = recent_readings.back()
                .map(|prev| similarity(&normalize(prev), &normalized, profile.consecutive_agreement_threshold) >= profile.consecutive_agreement_threshold)
                .unwrap_or(false);

            if agrees_with_recent {
                // Agreeing reading — add to the group
                recent_readings.push_back(cleaned.to_string());
                full_text_for_translation = text.clone();
                // Keep buffer bounded
                if recent_readings.len() > profile.required_agreements + 2 {
                    recent_readings.pop_front();
                }
            } else {
                // New/different text — reset the group
                recent_readings.clear();
                recent_readings.push_back(cleaned.to_string());
                full_text_for_translation = text.clone();
                group_start = Instant::now();
                group_translated = false;

                // Check if this is genuinely different from what we last translated
                if let Some(ref last) = last_translated_normalized {
                    if similarity(last, &normalized, profile.consecutive_agreement_threshold) >= profile.consecutive_agreement_threshold {
                        // Same as what we already translated — mark as done
                        group_translated = true;
                    }
                }
            }

            // Check if we have enough agreements + settle time
            if !group_translated && recent_readings.len() >= profile.required_agreements {
                let elapsed = group_start.elapsed().as_millis();
                if elapsed >= profile.min_settle_ms {
                    let to_translate = if full_text_for_translation.is_empty() {
                        recent_readings.back().unwrap().clone()
                    } else {
                        full_text_for_translation.clone()
                    };
                    if !is_duplicate(&to_translate, &translated_history, profile.similarity_threshold) {
                        log::info!("[tick {}] Text stable ({} agreements, {}ms), translating: {}",
                            tick_count, recent_readings.len(), elapsed,
                            truncate_chars(&to_translate, 120));
                        do_translate(backend, &to_translate, source_lang, target_lang, translated_text, &mut translated_history).await;
                        last_translated_normalized = Some(normalize(&to_translate));
                    } else {
                        log::debug!("[tick {}] Skipping duplicate stable text", tick_count);
                    }
                    group_translated = true;
                }
            }
        }
    }

    Ok(())
}

/// Check if normalized text is too similar to any recently translated text.
fn is_duplicate(normalized: &str, history: &[String], threshold: f64) -> bool {
    history.iter().any(|prev| similarity(prev, normalized, threshold) >= threshold)
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
