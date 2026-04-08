#[cfg(test)]
mod tests {
    use eyeclipse::config::*;
    use std::fs;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.hotkey, "Super+Shift+S");
        assert_eq!(config.source_lang, "ja");
        assert_eq!(config.target_lang, "en");
        assert_eq!(config.api_backend, ApiBackend::Deepl);
        assert_eq!(config.mode, TranslationMode::Oneshot);
        assert_eq!(config.live_interval_ms, 1000);
        assert_eq!(config.ocr_lang, "jpn+eng");
        assert!(config.api_key.is_empty());
        assert!(config.api_url.is_empty());
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let config = AppConfig {
            hotkey: "Ctrl+Shift+T".to_string(),
            source_lang: "en".to_string(),
            target_lang: "ja".to_string(),
            api_backend: ApiBackend::Openai,
            api_key: "test-key-123".to_string(),
            api_url: "https://custom.api.com".to_string(),
            mode: TranslationMode::Live,
            live_interval_ms: 500,
            ocr_lang: "eng".to_string(),
            settle_time_ms: 2000,
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.hotkey, "Ctrl+Shift+T");
        assert_eq!(deserialized.source_lang, "en");
        assert_eq!(deserialized.target_lang, "ja");
        assert_eq!(deserialized.api_backend, ApiBackend::Openai);
        assert_eq!(deserialized.api_key, "test-key-123");
        assert_eq!(deserialized.api_url, "https://custom.api.com");
        assert_eq!(deserialized.mode, TranslationMode::Live);
        assert_eq!(deserialized.live_interval_ms, 500);
        assert_eq!(deserialized.ocr_lang, "eng");
    }

    #[test]
    fn test_deserialize_with_missing_fields_uses_defaults() {
        let toml_str = r#"
api_key = "my-key"
target_lang = "de"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.api_key, "my-key");
        assert_eq!(config.target_lang, "de");
        // Defaults
        assert_eq!(config.hotkey, "Super+Shift+S");
        assert_eq!(config.source_lang, "ja");
        assert_eq!(config.api_backend, ApiBackend::Deepl);
        assert_eq!(config.mode, TranslationMode::Oneshot);
        assert_eq!(config.live_interval_ms, 1000);
    }

    #[test]
    fn test_deserialize_empty_string() {
        let toml_str = "";
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.hotkey, "Super+Shift+S");
    }

    #[test]
    fn test_save_and_load_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = AppConfig {
            api_key: "file-test-key".to_string(),
            target_lang: "fr".to_string(),
            ..AppConfig::default()
        };

        let content = toml::to_string_pretty(&config).unwrap();
        fs::write(&path, &content).unwrap();

        let loaded_content = fs::read_to_string(&path).unwrap();
        let loaded: AppConfig = toml::from_str(&loaded_content).unwrap();

        assert_eq!(loaded.api_key, "file-test-key");
        assert_eq!(loaded.target_lang, "fr");
        assert_eq!(loaded.source_lang, "ja");
    }

    #[test]
    fn test_all_backends_deserialize() {
        for (input, expected) in [
            ("\"deepl\"", ApiBackend::Deepl),
            ("\"libretranslate\"", ApiBackend::LibreTranslate),
            ("\"openai\"", ApiBackend::Openai),
        ] {
            let toml_str = format!("api_backend = {}", input);
            let config: AppConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(config.api_backend, expected);
        }
    }

    #[test]
    fn test_all_modes_deserialize() {
        for (input, expected) in [
            ("\"oneshot\"", TranslationMode::Oneshot),
            ("\"live\"", TranslationMode::Live),
        ] {
            let toml_str = format!("mode = {}", input);
            let config: AppConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(config.mode, expected);
        }
    }
}
