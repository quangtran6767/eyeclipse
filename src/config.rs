use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TranslationMode {
    Oneshot,
    Live,
}

impl Default for TranslationMode {
    fn default() -> Self {
        Self::Oneshot
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApiBackend {
    Deepl,
    LibreTranslate,
    Openai,
}

impl Default for ApiBackend {
    fn default() -> Self {
        Self::Deepl
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    #[serde(default = "default_source_lang")]
    pub source_lang: String,

    #[serde(default = "default_target_lang")]
    pub target_lang: String,

    #[serde(default)]
    pub api_backend: ApiBackend,

    #[serde(default)]
    pub api_key: String,

    #[serde(default)]
    pub api_url: String,

    #[serde(default)]
    pub mode: TranslationMode,

    #[serde(default = "default_live_interval_ms")]
    pub live_interval_ms: u64,

    #[serde(default = "default_ocr_lang")]
    pub ocr_lang: String,

    #[serde(default = "default_settle_time_ms")]
    pub settle_time_ms: u64,
}

fn default_hotkey() -> String {
    "Super+Shift+S".to_string()
}

fn default_source_lang() -> String {
    "ja".to_string()
}

fn default_target_lang() -> String {
    "en".to_string()
}

fn default_live_interval_ms() -> u64 {
    1000
}

fn default_ocr_lang() -> String {
    "jpn+eng".to_string()
}

fn default_settle_time_ms() -> u64 {
    1500
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            source_lang: default_source_lang(),
            target_lang: default_target_lang(),
            api_backend: ApiBackend::default(),
            api_key: String::new(),
            api_url: String::new(),
            mode: TranslationMode::default(),
            live_interval_ms: default_live_interval_ms(),
            ocr_lang: default_ocr_lang(),
            settle_time_ms: default_settle_time_ms(),
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("eyeclipse");
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
