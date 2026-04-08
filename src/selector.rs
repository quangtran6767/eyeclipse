use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Let the user draw a selection rectangle using `slop`.
/// Returns `Some(Region)` on success, or `None` if cancelled (Escape / right-click).
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
