use image::DynamicImage;

/// Compare two images and return true if they differ beyond the threshold.
///
/// `threshold` is a per-pixel absolute difference tolerance (0-255).
/// `change_ratio` is the fraction of pixels that must differ to trigger a change (0.0-1.0).
pub fn images_differ(a: &DynamicImage, b: &DynamicImage, threshold: u8, change_ratio: f64) -> bool {
    let a_rgba = a.to_rgba8();
    let b_rgba = b.to_rgba8();

    if a_rgba.dimensions() != b_rgba.dimensions() {
        return true;
    }

    let a_raw = a_rgba.as_raw();
    let b_raw = b_rgba.as_raw();

    // Fast path: exact match
    if a_raw == b_raw {
        return false;
    }

    let total_pixels = (a_rgba.width() * a_rgba.height()) as usize;
    let mut changed_pixels = 0usize;
    let required_changes = (total_pixels as f64 * change_ratio).ceil() as usize;

    for (pa, pb) in a_raw.chunks_exact(4).zip(b_raw.chunks_exact(4)) {
        let diff = pa
            .iter()
            .zip(pb.iter())
            .take(3) // compare RGB, skip alpha
            .any(|(a, b)| a.abs_diff(*b) > threshold);

        if diff {
            changed_pixels += 1;
            // Early exit once we know enough pixels changed
            if changed_pixels >= required_changes {
                return true;
            }
        }
    }

    changed_pixels >= required_changes
}
