use anyhow::{Context, Result};
use image::DynamicImage;
use xcap::Monitor;

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<DynamicImage> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;

    // Find the monitor that contains the selection origin
    let monitor = monitors
        .iter()
        .find(|m| {
            let mx = m.x();
            let my = m.y();
            let mw = m.width() as i32;
            let mh = m.height() as i32;
            x >= mx && y >= my && x < mx + mw && y < my + mh
        })
        .or_else(|| monitors.first())
        .context("No monitor found")?;

    let mon_x = monitor.x();
    let mon_y = monitor.y();

    log::debug!(
        "Capturing from monitor at ({}, {}), selection at ({}, {}) {}x{}",
        mon_x, mon_y, x, y, width, height
    );

    let full = monitor.capture_image().context("Failed to capture screen")?;
    let full_dyn = DynamicImage::ImageRgba8(full);

    // Convert absolute screen coords to monitor-relative coords
    let rel_x = (x - mon_x).max(0) as u32;
    let rel_y = (y - mon_y).max(0) as u32;

    let cropped = full_dyn.crop_imm(rel_x, rel_y, width, height);

    if cropped.width() == 0 || cropped.height() == 0 {
        anyhow::bail!(
            "Crop resulted in empty image: rel ({}, {}) {}x{} on {}x{} monitor",
            rel_x, rel_y, width, height,
            full_dyn.width(), full_dyn.height()
        );
    }

    Ok(cropped)
}

pub fn capture_full_screen() -> Result<DynamicImage> {
    let monitors = Monitor::all().context("Failed to enumerate monitors")?;
    let monitor = monitors.into_iter().next().context("No monitor found")?;
    let full = monitor.capture_image().context("Failed to capture screen")?;
    Ok(DynamicImage::ImageRgba8(full))
}
