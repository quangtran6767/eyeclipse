use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::TranslationBackend;

pub struct DeeplBackend {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Deserialize)]
struct DeeplResponse {
    translations: Vec<DeeplTranslation>,
}

#[derive(Deserialize)]
struct DeeplTranslation {
    text: String,
}

impl DeeplBackend {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let base_url = base_url.unwrap_or_else(|| {
            if api_key.ends_with(":fx") {
                "https://api-free.deepl.com".to_string()
            } else {
                "https://api.deepl.com".to_string()
            }
        });
        Self {
            client: Client::new(),
            api_key,
            base_url,
        }
    }
}

impl TranslationBackend for DeeplBackend {
    async fn translate(&self, text: String, _source: String, target: String) -> Result<String> {
        let url = format!("{}/v2/translate", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .json(&serde_json::json!({
                "text": [&text],
                "target_lang": target.to_uppercase()
            }))
            .send()
            .await
            .context("DeepL API request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("DeepL API error ({}): {}", status, body);
        }

        let data: DeeplResponse = resp.json().await.context("Failed to parse DeepL response")?;
        data.translations
            .into_iter()
            .next()
            .map(|t| t.text)
            .context("DeepL returned no translations")
    }
}
