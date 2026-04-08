#[cfg(test)]
mod tests {
    use eyeclipse::diff::images_differ;
    use image::{DynamicImage, RgbaImage};

    fn create_solid_image(width: u32, height: u32, r: u8, g: u8, b: u8) -> DynamicImage {
        let mut img = RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([r, g, b, 255]);
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_identical_images_no_diff() {
        let a = create_solid_image(100, 100, 128, 128, 128);
        let b = create_solid_image(100, 100, 128, 128, 128);
        assert!(!images_differ(&a, &b, 0, 0.01));
    }

    #[test]
    fn test_completely_different_images() {
        let a = create_solid_image(100, 100, 0, 0, 0);
        let b = create_solid_image(100, 100, 255, 255, 255);
        assert!(images_differ(&a, &b, 0, 0.01));
    }

    #[test]
    fn test_slight_difference_below_threshold() {
        let a = create_solid_image(100, 100, 128, 128, 128);
        let b = create_solid_image(100, 100, 130, 128, 128); // only 2 off in R
        assert!(!images_differ(&a, &b, 5, 0.01)); // threshold=5, so 2 is below
    }

    #[test]
    fn test_slight_difference_above_threshold() {
        let a = create_solid_image(100, 100, 128, 128, 128);
        let b = create_solid_image(100, 100, 140, 128, 128); // 12 off in R
        assert!(images_differ(&a, &b, 5, 0.01)); // threshold=5, 12 is above
    }

    #[test]
    fn test_different_dimensions_always_differ() {
        let a = create_solid_image(100, 100, 128, 128, 128);
        let b = create_solid_image(50, 50, 128, 128, 128);
        assert!(images_differ(&a, &b, 0, 0.01));
    }

    #[test]
    fn test_change_ratio_filters_sparse_changes() {
        let mut img_buf = RgbaImage::new(100, 100);
        for pixel in img_buf.pixels_mut() {
            *pixel = image::Rgba([128, 128, 128, 255]);
        }
        let a = DynamicImage::ImageRgba8(img_buf);

        // Change just 1 pixel out of 10000
        let mut img_buf2 = RgbaImage::new(100, 100);
        for pixel in img_buf2.pixels_mut() {
            *pixel = image::Rgba([128, 128, 128, 255]);
        }
        img_buf2.put_pixel(50, 50, image::Rgba([255, 0, 0, 255]));
        let b = DynamicImage::ImageRgba8(img_buf2);

        // 1 pixel = 0.01% of 10000, change_ratio=1% requires 100 pixels
        assert!(!images_differ(&a, &b, 0, 0.01));

        // But with a very low ratio, it should detect
        assert!(images_differ(&a, &b, 0, 0.0001));
    }

    #[test]
    fn test_zero_threshold_exact_match() {
        let a = create_solid_image(10, 10, 100, 100, 100);
        let b = create_solid_image(10, 10, 101, 100, 100); // 1 off
        // threshold=0 means any difference counts
        assert!(images_differ(&a, &b, 0, 0.01));
    }

    #[test]
    fn test_empty_images() {
        let a = create_solid_image(1, 1, 0, 0, 0);
        let b = create_solid_image(1, 1, 0, 0, 0);
        assert!(!images_differ(&a, &b, 0, 0.5));
    }
}
