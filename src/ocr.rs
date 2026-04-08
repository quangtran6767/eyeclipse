use anyhow::{Context, Result};
use image::DynamicImage;
use leptess::{LepTess, Variable};
use std::io::Cursor;

pub struct OcrResult {
    pub text: String,
    pub confidence: i32,
}

pub fn extract_text(img: &DynamicImage, lang: &str) -> Result<OcrResult> {
    // Preprocess: convert to grayscale
    let mut gray = img.to_luma8();

    // Upscale if either dimension is small (Tesseract works best ≥300 DPI equivalent)
    let (w, h) = gray.dimensions();
    if w < 600 || h < 40 {
        let scale = if w < 300 || h < 20 { 4 } else { 2 };
        gray = image::imageops::resize(
            &gray,
            w * scale,
            h * scale,
            image::imageops::FilterType::Lanczos3,
        );
    }

    // Binarize: threshold to isolate light text (subtitles) from dark/busy backgrounds.
    // Pixels >= 180 become white (255), everything else becomes black (0).
    // This dramatically reduces noise from video backgrounds.
    for pixel in gray.pixels_mut() {
        pixel.0[0] = if pixel.0[0] >= 180 { 255 } else { 0 };
    }

    let preprocessed = DynamicImage::ImageLuma8(gray);

    // Encode as PNG for leptess
    let mut png_bytes: Vec<u8> = Vec::new();
    preprocessed
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .context("Failed to encode image as PNG for OCR")?;

    // Run OCR in a catch_unwind to survive panics from the C library
    let lang_owned = lang.to_string();
    let ocr_result = std::panic::catch_unwind(move || -> Result<OcrResult> {
        let mut lt = LepTess::new(None, &lang_owned)
            .context("Failed to init Tesseract")?;

        // PSM 6 = "Assume a single uniform block of text"
        lt.set_variable(Variable::TesseditPagesegMode, "6")
            .context("Failed to set PSM")?;

        // Restrict recognized characters to match the configured language.
        if let Some(whitelist) = build_char_whitelist(&lang_owned) {
            lt.set_variable(Variable::TesseditCharWhitelist, &whitelist)
                .context("Failed to set char whitelist")?;
        }

        lt.set_image_from_mem(&png_bytes)
            .context("Failed to set image for OCR")?;
        let raw = lt.get_utf8_text().context("Failed to extract text")?;
        let confidence = lt.mean_text_conf();

        // Post-process: strip noise lines, keep only meaningful text
        let cleaned = clean_ocr_output(&raw, &lang_owned);
        Ok(OcrResult { text: cleaned, confidence })
    });

    match ocr_result {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Tesseract crashed (panic) during OCR — try a different region"),
    }
}

/// Remove noise lines from OCR output. Keeps only lines that look like real text.
fn clean_ocr_output(raw: &str, lang: &str) -> String {
    let is_cjk = is_cjk_lang(lang);

    let lines: Vec<&str> = raw.lines().collect();
    let kept: Vec<&str> = lines
        .into_iter()
        .map(|l| l.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| {
            if is_cjk {
                is_meaningful_cjk_line(line)
            } else {
                is_meaningful_latin_line(line)
            }
        })
        .collect();

    if is_cjk {
        // Japanese/Chinese: remove all ASCII spaces (they're OCR noise in CJK text)
        kept.join("\n")
            .chars()
            .filter(|c| *c != ' ')
            .collect::<String>()
            .trim()
            .to_string()
    } else {
        kept.join("\n").trim().to_string()
    }
}

/// Check if a line contains meaningful CJK text (Japanese/Chinese/Korean).
/// Rejects lines that are mostly symbols, numbers, single chars, or Latin noise.
fn is_meaningful_cjk_line(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 2 {
        return false;
    }

    // Reject lines containing pipe '|' — almost always OCR noise, never in subtitles
    if chars.contains(&'|') {
        return false;
    }

    // Count meaningful CJK characters: hiragana, katakana, kanji, hangul
    let meaningful = chars.iter().filter(|c| is_cjk_char(**c)).count();
    let total_non_space = chars.iter().filter(|c| !c.is_whitespace()).count();

    if total_non_space == 0 {
        return false;
    }

    // Reject lines dominated by katakana long-vowel mark (ー) — common OCR noise
    let longvowel_count = chars.iter().filter(|c| **c == 'ー').count();
    if longvowel_count > 0 && longvowel_count as f64 / total_non_space as f64 >= 0.3 {
        return false;
    }

    // Reject lines with too many small katakana (ュ, ョ, ャ, ッ) relative to content
    // These are common OCR noise artifacts
    let small_katakana_count = chars.iter().filter(|c| matches!(**c, 'ュ' | 'ョ' | 'ャ' | 'ッ' | 'ェ')).count();
    if small_katakana_count > 0 && small_katakana_count as f64 / total_non_space as f64 >= 0.3 {
        return false;
    }

    // At least 40% of non-space chars should be CJK, and at least 4 CJK chars total
    let ratio = meaningful as f64 / total_non_space as f64;
    meaningful >= 4 && ratio >= 0.4
}

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        // Hiragana
        '\u{3040}'..='\u{309F}' |
        // Katakana
        '\u{30A0}'..='\u{30FF}' |
        // CJK Unified Ideographs (kanji)
        '\u{4E00}'..='\u{9FFF}' |
        // CJK Extension A
        '\u{3400}'..='\u{4DBF}' |
        // Hangul
        '\u{AC00}'..='\u{D7AF}' |
        // Fullwidth digits/letters (Japanese uses these)
        '\u{FF01}'..='\u{FF60}' |
        // CJK punctuation
        '\u{3000}'..='\u{303F}' |
        // Halfwidth katakana
        '\u{FF65}'..='\u{FF9F}'
    )
}

/// Check if a line looks like meaningful Latin-script text.
fn is_meaningful_latin_line(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 3 {
        return false;
    }

    let alpha = chars.iter().filter(|c| c.is_alphabetic()).count();
    let total_non_space = chars.iter().filter(|c| !c.is_whitespace()).count();

    if total_non_space == 0 {
        return false;
    }

    // At least 50% alphabetic characters
    let ratio = alpha as f64 / total_non_space as f64;
    if alpha < 2 || ratio < 0.5 {
        return false;
    }

    // Reject lines with very low average word length (<2 chars per word)
    let words: Vec<&str> = line.split_whitespace().collect();
    if !words.is_empty() {
        let avg_word_len: f64 = words.iter().map(|w| w.len() as f64).sum::<f64>() / words.len() as f64;
        if avg_word_len < 2.0 {
            return false;
        }
    }

    true
}

/// Build a character whitelist string based on the OCR language config.
/// Returns None for CJK languages (whitelist would be too large / counterproductive).
fn build_char_whitelist(lang: &str) -> Option<String> {
    let langs: Vec<&str> = lang.split('+').collect();

    // If any CJK language is present, don't whitelist — too many chars
    let has_cjk = langs.iter().any(|l| {
        matches!(*l, "jpn" | "chi_sim" | "chi_tra" | "kor" | "chi_sim_vert" | "chi_tra_vert" | "jpn_vert")
    });
    if has_cjk {
        return None;
    }

    // For Latin-script languages, allow ASCII printable + common accented chars
    let has_latin = langs.iter().any(|l| {
        matches!(*l, "eng" | "fra" | "deu" | "spa" | "por" | "ita" | "nld" | "vie"
            | "pol" | "tur" | "ron" | "ces" | "slk" | "hrv" | "hun" | "swe"
            | "nor" | "dan" | "fin" | "ind" | "msa" | "cat" | "eus")
    });

    if has_latin {
        let mut chars = String::new();
        // ASCII printable
        for c in ' '..='~' {
            chars.push(c);
        }
        // Common accented Latin characters (covers Vietnamese, French, German, Spanish, etc.)
        chars.push_str("àáâãäåæçèéêëìíîïðñòóôõöøùúûüýþÿ");
        chars.push_str("ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖØÙÚÛÜÝÞ");
        // Vietnamese-specific diacritics
        chars.push_str("ăắằẳẵặđĐơớờởỡợưứừửữự");
        chars.push_str("ĂẮẰẲẴẶƠỚỜỞỠỢƯỨỪỬỮỰ");
        chars.push_str("ạảấầẩẫậẹẻẽếềểễệỉịọỏốồổỗộụủứừửữựỳỵỷỹ");
        chars.push_str("ẠẢẤẦẨẪẬẸẺẼẾỀỂỄỆỈỊỌỎỐỒỔỖỘỤỦỨỪỬỮỰỲỴỶỸ");
        // Newline/tab
        chars.push('\n');
        chars.push('\t');
        return Some(chars);
    }

    // For Cyrillic languages
    let has_cyrillic = langs.iter().any(|l| {
        matches!(*l, "rus" | "ukr" | "bel" | "bul" | "srp" | "mkd")
    });
    if has_cyrillic {
        let mut chars = String::new();
        for c in ' '..='~' {
            chars.push(c);
        }
        // Cyrillic block
        for c in 'А'..='я' {
            chars.push(c);
        }
        chars.push_str("ёЁ");
        // Ukrainian
        chars.push_str("іїєґІЇЄҐ");
        chars.push('\n');
        chars.push('\t');
        return Some(chars);
    }

    // For Arabic/Thai/other scripts — don't whitelist
    None
}

pub fn check_tesseract_available(lang: &str) -> Result<()> {
    LepTess::new(None, lang).context(format!(
        "Tesseract not available for language '{}'. \
         Install tesseract-ocr and the language pack (e.g. tesseract-ocr-{}).",
        lang,
        lang.split('+').next().unwrap_or(lang)
    ))?;
    Ok(())
}

/// Check if OCR output looks like garbage (log lines, IDE UI, file paths, etc.)
/// rather than actual subtitle/content text.
/// Returns true if the text should be rejected.
pub fn is_garbage_text(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }

    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return true;
    }

    let garbage_count = lines.iter().filter(|line| is_garbage_line(line)).count();
    let ratio = garbage_count as f64 / lines.len() as f64;

    // If more than 30% of lines look like garbage, reject the whole thing
    ratio > 0.3
}

fn is_garbage_line(line: &str) -> bool {
    let lower = line.to_lowercase();

    // Log patterns
    if lower.contains("[20") && (lower.contains("info") || lower.contains("warn") || lower.contains("error")) {
        return true;
    }

    // Eyeclipse log feedback loop — overlay/terminal log text being re-captured by OCR.
    // OCR often misreads "eyeclipse" as "clipse", "::live]" as "::live [", etc.
    if lower.contains("eyeclipse") || lower.contains("clipse") {
        return true;
    }
    if lower.contains("::live") {
        return true;
    }
    if lower.contains("[tick") {
        return true;
    }
    if lower.contains("text stable") {
        return true;
    }
    if lower.contains("translating:") {
        return true;
    }
    if lower.contains("agreements") {
        return true;
    }
    if lower.contains("ocr:") || lower.contains("ocr (") {
        return true;
    }
    if lower.contains("lck") {
        return true;
    }
    if lower.contains("translation (") && lower.contains("ms)") {
        return true;
    }

    // File/path patterns
    if lower.contains("~/") || lower.contains(".log") || lower.contains("projects/") {
        return true;
    }

    // IDE UI patterns
    if lower.contains("debugconsole") || lower.contains("gitlens") {
        return true;
    }
    let ide_keywords = ["problems", "output", "terminal", "debugconsol"];
    let ide_matches = ide_keywords.iter().filter(|kw| lower.contains(**kw)).count();
    if ide_matches >= 2 {
        return true;
    }

    // OCR self-reference patterns (log format leaking)
    if lower.contains("primary:") || lower.contains("| primary") {
        return true;
    }

    false
}

/// Check if the OCR language config is CJK.
pub fn is_cjk_lang(lang: &str) -> bool {
    lang.split('+').any(|l| {
        matches!(l, "jpn" | "chi_sim" | "chi_tra" | "kor" | "jpn_vert" | "chi_sim_vert" | "chi_tra_vert")
    })
}

/// Strip leading OCR noise characters from a line.
/// Tesseract often prepends random punctuation, numbers, or single chars
/// to otherwise clean subtitle text (e.g., "、どんな子供..." or "4大人に...").
pub fn strip_leading_noise(line: &str) -> &str {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    if chars.is_empty() {
        return line;
    }

    // Find where the "real" content starts by skipping leading noise chars:
    // punctuation, digits, ASCII symbols, single-char CJK noise, long-vowel marks
    let mut start_idx = 0;
    for &(byte_idx, c) in &chars {
        if c.is_ascii_punctuation()
            || c.is_ascii_digit()
            || c.is_ascii_whitespace()
            || c == 'ー'
            || matches!(c, '、' | '。' | '」' | '「' | '』' | '『' | '）' | '（'
                | '，' | '．' | '・' | '〜' | '～' | '"' | '"'
                | '【' | '】' | '〔' | '〕')
        {
            start_idx = byte_idx + c.len_utf8();
        } else {
            break;
        }
    }

    if start_idx >= line.len() {
        return line; // All noise? Return original to avoid empty string
    }

    &line[start_idx..]
}
