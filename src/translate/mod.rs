pub mod deepl;
pub mod libretranslate;
pub mod openai;

use anyhow::Result;
use crate::config::{ApiBackend, AppConfig};

pub trait TranslationBackend: Send + Sync {
    fn translate(
        &self,
        text: String,
        source: String,
        target: String,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
}

pub fn create_backend(config: &AppConfig) -> Box<dyn TranslationBackendDyn> {
    match config.api_backend {
        ApiBackend::Deepl => Box::new(deepl::DeeplBackend::new(
            config.api_key.clone(),
            if config.api_url.is_empty() {
                None
            } else {
                Some(config.api_url.clone())
            },
        )),
        ApiBackend::LibreTranslate => Box::new(libretranslate::LibreTranslateBackend::new(
            if config.api_url.is_empty() {
                None
            } else {
                Some(config.api_url.clone())
            },
            if config.api_key.is_empty() {
                None
            } else {
                Some(config.api_key.clone())
            },
        )),
        ApiBackend::Openai => Box::new(openai::OpenAIBackend::new(
            config.api_key.clone(),
            if config.api_url.is_empty() {
                None
            } else {
                Some(config.api_url.clone())
            },
        )),
    }
}

/// Object-safe version of TranslationBackend for dynamic dispatch.
pub trait TranslationBackendDyn: Send + Sync {
    fn translate_dyn(
        &self,
        text: String,
        source: String,
        target: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>>;
}

// Blanket impl for anything implementing the static trait
impl<T: TranslationBackend + Sync> TranslationBackendDyn for T {
    fn translate_dyn(
        &self,
        text: String,
        source: String,
        target: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        Box::pin(self.translate(text, source, target))
    }
}
