#[cfg(test)]
mod tests {
    use eyeclipse::settings::detect_tesseract_langs;
    use eyeclipse::selector;

    #[test]
    fn test_detect_tesseract_langs_returns_vec() {
        let langs = detect_tesseract_langs();
        // Should return at least one language if tesseract is installed,
        // or ["eng"] as fallback if not.
        assert!(!langs.is_empty(), "detect_tesseract_langs should never return empty");
    }

    #[test]
    fn test_detect_tesseract_langs_no_header_lines() {
        let langs = detect_tesseract_langs();
        for lang in &langs {
            assert!(
                !lang.contains("List of"),
                "Should not contain header text, got: {}",
                lang
            );
            assert!(!lang.is_empty(), "Should not contain empty strings");
        }
    }

    #[test]
    fn test_detect_tesseract_langs_has_eng() {
        let langs = detect_tesseract_langs();
        assert!(
            langs.iter().any(|l| l == "eng"),
            "Should detect 'eng' language pack, got: {:?}",
            langs
        );
    }

    #[test]
    fn test_select_region_struct_fields() {
        let region = selector::Region {
            x: 100,
            y: 200,
            width: 300,
            height: 400,
        };
        assert_eq!(region.x, 100);
        assert_eq!(region.y, 200);
        assert_eq!(region.width, 300);
        assert_eq!(region.height, 400);
    }

    #[test]
    fn test_config_ocr_lang_splitting() {
        // Test that the OCR lang string splitting logic used in settings works
        let ocr_lang = "jpn+eng+chi_sim";
        let parts: Vec<&str> = ocr_lang.split('+').collect();
        assert_eq!(parts, vec!["jpn", "eng", "chi_sim"]);
    }

    #[test]
    fn test_config_ocr_lang_joining() {
        let selected = vec!["eng", "jpn"];
        let joined = selected.join("+");
        assert_eq!(joined, "eng+jpn");
    }

    #[test]
    fn test_config_ocr_lang_empty_join() {
        let selected: Vec<&str> = vec![];
        let joined = selected.join("+");
        assert_eq!(joined, "");
    }

    #[test]
    fn test_config_target_lang_codes() {
        // Verify common language codes are valid non-empty strings
        let codes = ["en", "vi", "ja", "zh", "ko", "fr", "de", "es", "pt", "ru"];
        for code in &codes {
            assert!(!code.is_empty());
            assert!(code.len() <= 4);
            assert!(code.chars().all(|c| c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn test_config_save_reload_preserves_target_lang() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = eyeclipse::config::AppConfig {
            target_lang: "vi".to_string(),
            ocr_lang: "eng+jpn".to_string(),
            ..eyeclipse::config::AppConfig::default()
        };

        let content = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&path, &content).unwrap();

        let loaded_content = std::fs::read_to_string(&path).unwrap();
        let loaded: eyeclipse::config::AppConfig = toml::from_str(&loaded_content).unwrap();

        assert_eq!(loaded.target_lang, "vi");
        assert_eq!(loaded.ocr_lang, "eng+jpn");
    }
}
