use anyhow::{Context, Result};
use image::DynamicImage;
use xcap::Monitor;

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<DynamicImage> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;
    let monitor = monitors
        .into_iter()
        .next()
        .context("No monitor found")?;

    let full = monitor.capture_image().context("Failed to capture screen")?;
    let full_dyn = DynamicImage::ImageRgba8(full);

    let cropped = full_dyn.crop_imm(
        x.max(0) as u32,
        y.max(0) as u32,
        width,
        height,
    );

    Ok(cropped)
}

pub fn capture_full_screen() -> Result<DynamicImage> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;
    let monitor = monitors.into_iter().next().context("No monitor found")?;
    let full = monitor.capture_image().context("Failed to capture screen")?;
    Ok(DynamicImage::ImageRgba8(full))
}
