use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::TranslationBackend;

pub struct LibreTranslateBackend {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct LTResponse {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

impl LibreTranslateBackend {
    pub fn new(base_url: Option<String>, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.unwrap_or_else(|| "http://localhost:5000".to_string()),
            api_key,
        }
    }
}

impl TranslationBackend for LibreTranslateBackend {
    async fn translate(&self, text: String, source: String, target: String) -> Result<String> {
        let url = format!("{}/translate", self.base_url);

        let mut body = serde_json::json!({
            "q": &text,
            "source": &source,
            "target": &target,
            "format": "text"
        });

        if let Some(key) = &self.api_key {
            body["api_key"] = serde_json::Value::String(key.clone());
        }

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("LibreTranslate API request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LibreTranslate error ({}): {}", status, err_body);
        }

        let data: LTResponse = resp.json().await.context("Failed to parse LibreTranslate response")?;
        Ok(data.translated_text)
    }
}
