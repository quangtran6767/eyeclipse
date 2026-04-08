use anyhow::{Context, Result};
use image::DynamicImage;
use leptess::LepTess;
use std::io::Cursor;

pub fn extract_text(img: &DynamicImage, lang: &str) -> Result<String> {
    // Encode image as PNG bytes — leptess set_image_from_mem expects encoded image data
    let mut png_bytes: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .context("Failed to encode image as PNG for OCR")?;

    let mut lt = LepTess::new(None, lang).context("Failed to init Tesseract. Is tesseract-ocr installed?")?;

    lt.set_image_from_mem(&png_bytes)
        .context("Failed to set image for OCR")?;

    let text = lt.get_utf8_text().context("Failed to extract text via OCR")?;

    Ok(text.trim().to_string())
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
