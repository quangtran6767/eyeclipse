#[cfg(test)]
mod tests {
    use mockito::Server;

    #[tokio::test]
    async fn test_deepl_translate_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/v2/translate")
            .match_header("Authorization", "DeepL-Auth-Key test-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"translations":[{"detected_source_language":"JA","text":"Hello world"}]}"#)
            .create_async()
            .await;

        let backend = eyeclipse::translate::deepl::DeeplBackend::new(
            "test-key".to_string(),
            Some(server.url()),
        );

        use eyeclipse::translate::TranslationBackend;
        let result = backend
            .translate("こんにちは世界".to_string(), "ja".to_string(), "en".to_string())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello world");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_deepl_translate_api_error() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/v2/translate")
            .with_status(403)
            .with_body("Forbidden")
            .create_async()
            .await;

        let backend = eyeclipse::translate::deepl::DeeplBackend::new(
            "bad-key".to_string(),
            Some(server.url()),
        );

        use eyeclipse::translate::TranslationBackend;
        let result = backend
            .translate("test".to_string(), "ja".to_string(), "en".to_string())
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("403"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_libretranslate_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/translate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"translatedText":"Bonjour le monde"}"#)
            .create_async()
            .await;

        let backend = eyeclipse::translate::libretranslate::LibreTranslateBackend::new(
            Some(server.url()),
            None,
        );

        use eyeclipse::translate::TranslationBackend;
        let result = backend
            .translate("Hello world".to_string(), "en".to_string(), "fr".to_string())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Bonjour le monde");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_libretranslate_error() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/translate")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let backend = eyeclipse::translate::libretranslate::LibreTranslateBackend::new(
            Some(server.url()),
            None,
        );

        use eyeclipse::translate::TranslationBackend;
        let result = backend
            .translate("test".to_string(), "en".to_string(), "fr".to_string())
            .await;

        assert!(result.is_err());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_openai_translate_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("Authorization", "Bearer test-openai-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[{"message":{"content":"Translated text here"}}]}"#)
            .create_async()
            .await;

        let backend = eyeclipse::translate::openai::OpenAIBackend::new(
            "test-openai-key".to_string(),
            Some(server.url()),
        );

        use eyeclipse::translate::TranslationBackend;
        let result = backend
            .translate("Original text".to_string(), "en".to_string(), "ja".to_string())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Translated text here");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_openai_translate_error() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(429)
            .with_body("Rate limited")
            .create_async()
            .await;

        let backend = eyeclipse::translate::openai::OpenAIBackend::new(
            "key".to_string(),
            Some(server.url()),
        );

        use eyeclipse::translate::TranslationBackend;
        let result = backend
            .translate("text".to_string(), "en".to_string(), "ja".to_string())
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("429"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_backend_dyn_dispatch() {
        let mut server = Server::new_async().await;

        server
            .mock("POST", "/v2/translate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"translations":[{"text":"dynamic dispatch works"}]}"#)
            .create_async()
            .await;

        let backend: Box<dyn eyeclipse::translate::TranslationBackendDyn> =
            Box::new(eyeclipse::translate::deepl::DeeplBackend::new(
                "key".to_string(),
                Some(server.url()),
            ));

        let result = backend
            .translate_dyn("test".to_string(), "ja".to_string(), "en".to_string())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "dynamic dispatch works");
    }
}
