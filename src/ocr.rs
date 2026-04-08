use anyhow::{Context, Result};
use image::DynamicImage;
use leptess::LepTess;
use std::io::Cursor;

pub fn extract_text(img: &DynamicImage, lang: &str) -> Result<String> {
    // Preprocess: convert to grayscale luma and upscale small images for better OCR
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

pub fn check_tesseract_available(lang: &str) -> Result<()> {
    LepTess::new(None, lang).context(format!(
        "Tesseract not available for language '{}'. \
         Install tesseract-ocr and the language pack (e.g. tesseract-ocr-{}).",
        lang,
        lang.split('+').next().unwrap_or(lang)
    ))?;
    Ok(())
}
