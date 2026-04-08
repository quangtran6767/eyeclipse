use anyhow::{Context, Result};
use image::DynamicImage;
use leptess::{LepTess, Variable};
use std::io::Cursor;

pub fn extract_text(img: &DynamicImage, lang: &str) -> Result<String> {
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
    let ocr_result = std::panic::catch_unwind(move || -> Result<String> {
        let mut lt = LepTess::new(None, &lang_owned)
            .context("Failed to init Tesseract")?;

        // PSM 6 = "Assume a single uniform block of text"
        // Much better for subtitles than the default (auto-detect layout)
        lt.set_variable(Variable::TesseditPagesegMode, "6")
            .context("Failed to set PSM")?;

        // Restrict recognized characters to match the configured language.
        // This prevents Tesseract from hallucinating random scripts on noisy backgrounds.
        if let Some(whitelist) = build_char_whitelist(&lang_owned) {
            lt.set_variable(Variable::TesseditCharWhitelist, &whitelist)
                .context("Failed to set char whitelist")?;
        }

        lt.set_image_from_mem(&png_bytes)
            .context("Failed to set image for OCR")?;
        let text = lt.get_utf8_text().context("Failed to extract text")?;
        Ok(text.trim().to_string())
    });

    match ocr_result {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Tesseract crashed (panic) during OCR — try a different region"),
    }
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
