use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::TranslationBackend;

pub struct OpenAIBackend {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

impl OpenAIBackend {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com".to_string()),
        }
    }
}

impl TranslationBackend for OpenAIBackend {
    async fn translate(&self, text: String, source: String, target: String) -> Result<String> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let system_prompt = format!(
            "You are a translator. Translate the following text from {} to {}. \
             Output ONLY the translated text, nothing else.",
            &source, &target
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": "gpt-4o-mini",
                "temperature": 0.0,
                "messages": [
                    {"role": "system", "content": &system_prompt},
                    {"role": "user", "content": &text}
                ]
            }))
            .send()
            .await
            .context("OpenAI API request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, body);
        }

        let data: ChatResponse = resp.json().await.context("Failed to parse OpenAI response")?;
        data.choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .context("OpenAI returned no choices")
    }
}
