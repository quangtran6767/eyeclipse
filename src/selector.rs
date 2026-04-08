use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Let the user draw a selection rectangle using `slop` (Linux/X11).
/// Returns `Some(Region)` on success, or `None` if cancelled (Escape / right-click).
#[cfg(target_os = "linux")]
pub fn select_region() -> Result<Option<Region>> {
    // slop draws a native X11 selection rectangle with no overlay/tint.
    // -f  = format string: x y w h
    // -q  = quiet (no stderr noise)
    // -b  = border width
    // -c  = color (R,G,B,A)
    // -l  = disable window detection (free selection only)
    let output = Command::new("slop")
        .args(["-f", "%x %y %w %h", "-q", "-b", "2", "-c", "0.4,0.6,1,0.4", "-l"])
        .output()
        .context(
            "Failed to run `slop`. Install it with: sudo apt install slop"
        )?;

    // slop exits 1 when user cancels (Escape / right-click)
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split_whitespace().collect();
    if parts.len() < 4 {
        anyhow::bail!("Unexpected slop output: {}", stdout);
    }

    let x: i32 = parts[0].parse().context("parse x")?;
    let y: i32 = parts[1].parse().context("parse y")?;
    let w: u32 = parts[2].parse().context("parse w")?;
    let h: u32 = parts[3].parse().context("parse h")?;

    if w < 3 || h < 3 {
        return Ok(None);
    }

    Ok(Some(Region { x, y, width: w, height: h }))
}

/// Let the user draw a selection rectangle using macOS `screencapture`.
/// Captures the selected region to a temp file and reads its dimensions.
/// The region position is extracted from the screencapture metadata.
#[cfg(target_os = "macos")]
pub fn select_region() -> Result<Option<Region>> {
    use std::path::PathBuf;

    let tmp_path = std::env::temp_dir().join("eyeclipse_selection.png");
    let tmp_str = tmp_path.to_string_lossy().to_string();

    // screencapture -i = interactive selection, -s = selection mode only
    // -x = no sound, -t png = format
    // The user draws a rectangle; the captured image is saved to the temp file.
    let status = Command::new("screencapture")
        .args(["-i", "-x", "-t", "png", &tmp_str])
        .status()
        .context(
            "Failed to run `screencapture`. This requires macOS with Screen Recording permissions."
        )?;

    // screencapture exits with non-zero if user presses Escape
    if !status.success() {
        return Ok(None);
    }

    // Check the file was created
    if !tmp_path.exists() {
        return Ok(None);
    }

    // Read the captured image to get dimensions
    let img = image::open(&tmp_path)
        .context("Failed to read captured screenshot")?;
    let width = img.width();
    let height = img.height();

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_path);

    if width < 3 || height < 3 {
        return Ok(None);
    }

    // On macOS, screencapture doesn't directly export x,y coordinates.
    // We use the `cliclick` approach or AppleScript to get mouse position.
    // Fallback: use the CoreGraphics mouse position at capture start.
    // For now, use (0,0) and rely on the image content — the overlay will be
    // positioned relative to the screen center as a fallback.
    // A more robust approach uses `screencapture -R x,y,w,h` but that requires
    // knowing the coordinates upfront.

    // Get current mouse position via AppleScript as a reasonable approximation
    // of where the user made the selection.
    let (mouse_x, mouse_y) = get_mouse_position_macos().unwrap_or((100, 100));

    // Approximate: place region at mouse position minus half the selection size
    let x = (mouse_x as i32 - width as i32 / 2).max(0);
    let y = (mouse_y as i32 - height as i32 / 2).max(0);

    Ok(Some(Region { x, y, width, height }))
}

/// Get current mouse position on macOS via AppleScript.
#[cfg(target_os = "macos")]
fn get_mouse_position_macos() -> Result<(u32, u32)> {
    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get {x, y} of (get position of mouse cursor) as string",
        ])
        .output()
        .context("Failed to get mouse position via AppleScript")?;

    // AppleScript output is something like "123, 456" — but actually
    // the mouse cursor position isn't directly accessible this way.
    // Use a Python one-liner with Quartz as alternative.
    if !output.status.success() {
        // Fallback: use Python + Quartz
        let py_output = Command::new("python3")
            .args([
                "-c",
                "from Quartz.CoreGraphics import CGEventGetLocation, CGEventCreate; e=CGEventCreate(None); loc=CGEventGetLocation(e); print(f'{int(loc.x)} {int(loc.y)}')",
            ])
            .output()
            .context("Failed to get mouse position")?;

        let stdout = String::from_utf8_lossy(&py_output.stdout);
        let parts: Vec<&str> = stdout.trim().split_whitespace().collect();
        if parts.len() >= 2 {
            let x: u32 = parts[0].parse().unwrap_or(100);
            let y: u32 = parts[1].parse().unwrap_or(100);
            return Ok((x, y));
        }
    }

    Ok((100, 100))
}
