use anyhow::Result;
use eframe::egui;
use std::process::Command;

use crate::config::{ApiBackend, AppConfig, TranslationMode};

/// Detect installed Tesseract language packs by running `tesseract --list-langs`.
pub fn detect_tesseract_langs() -> Vec<String> {
    let output = Command::new("tesseract")
        .arg("--list-langs")
        .output();

    match output {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stderr).to_string()
                + &String::from_utf8_lossy(&o.stdout);
            text.lines()
                .skip(1) // first line is "List of available languages"
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.contains("List of"))
                .collect()
        }
        Err(_) => vec!["eng".to_string()],
    }
}

/// Common translation target languages (display name, API code).
const TARGET_LANGS: &[(&str, &str)] = &[
    ("English", "en"),
    ("Vietnamese", "vi"),
    ("Japanese", "ja"),
    ("Chinese (Simplified)", "zh"),
    ("Korean", "ko"),
    ("French", "fr"),
    ("German", "de"),
    ("Spanish", "es"),
    ("Portuguese", "pt"),
    ("Russian", "ru"),
    ("Thai", "th"),
    ("Indonesian", "id"),
    ("Italian", "it"),
    ("Dutch", "nl"),
    ("Arabic", "ar"),
];

/// Common source languages.
const SOURCE_LANGS: &[(&str, &str)] = &[
    ("Japanese", "ja"),
    ("English", "en"),
    ("Chinese (Simplified)", "zh"),
    ("Korean", "ko"),
    ("Vietnamese", "vi"),
    ("French", "fr"),
    ("German", "de"),
    ("Spanish", "es"),
    ("Portuguese", "pt"),
    ("Russian", "ru"),
    ("Auto-detect", "auto"),
];

struct SettingsApp {
    config: AppConfig,
    available_ocr_langs: Vec<String>,
    selected_ocr_langs: Vec<bool>,
    saved: bool,
}

impl SettingsApp {
    fn new(config: AppConfig, available_ocr_langs: Vec<String>) -> Self {
        let current_ocr: Vec<&str> = config.ocr_lang.split('+').collect();
        let selected_ocr_langs: Vec<bool> = available_ocr_langs
            .iter()
            .map(|l| current_ocr.contains(&l.as_str()))
            .collect();

        Self {
            config,
            available_ocr_langs,
            selected_ocr_langs,
            saved: false,
        }
    }

    fn build_ocr_lang_string(&self) -> String {
        self.available_ocr_langs
            .iter()
            .zip(&self.selected_ocr_langs)
            .filter(|(_, &selected)| selected)
            .map(|(lang, _)| lang.as_str())
            .collect::<Vec<_>>()
            .join("+")
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Eyeclipse Settings");
            ui.separator();
            ui.add_space(8.0);

            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    // --- Translation Backend ---
                    ui.label("API Backend:");
                    let backend_label = match self.config.api_backend {
                        ApiBackend::Deepl => "DeepL",
                        ApiBackend::LibreTranslate => "LibreTranslate",
                        ApiBackend::Openai => "OpenAI",
                    };
                    egui::ComboBox::from_id_salt("api_backend")
                        .selected_text(backend_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.api_backend, ApiBackend::Deepl, "DeepL");
                            ui.selectable_value(&mut self.config.api_backend, ApiBackend::LibreTranslate, "LibreTranslate");
                            ui.selectable_value(&mut self.config.api_backend, ApiBackend::Openai, "OpenAI");
                        });
                    ui.end_row();

                    // --- API Key ---
                    ui.label("API Key:");
                    ui.add(egui::TextEdit::singleline(&mut self.config.api_key).desired_width(280.0).password(true));
                    ui.end_row();

                    // --- API URL ---
                    ui.label("API URL (optional):");
                    ui.add(egui::TextEdit::singleline(&mut self.config.api_url).desired_width(280.0).hint_text("Leave empty for default"));
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // --- Source Language ---
                    ui.label("Source Language:");
                    let src_display = SOURCE_LANGS
                        .iter()
                        .find(|(_, code)| *code == self.config.source_lang)
                        .map(|(name, _)| *name)
                        .unwrap_or(&self.config.source_lang);
                    egui::ComboBox::from_id_salt("source_lang")
                        .selected_text(src_display)
                        .show_ui(ui, |ui| {
                            for (name, code) in SOURCE_LANGS {
                                ui.selectable_value(&mut self.config.source_lang, code.to_string(), *name);
                            }
                        });
                    ui.end_row();

                    // --- Target Language ---
                    ui.label("Target Language:");
                    let tgt_display = TARGET_LANGS
                        .iter()
                        .find(|(_, code)| *code == self.config.target_lang)
                        .map(|(name, _)| *name)
                        .unwrap_or(&self.config.target_lang);
                    egui::ComboBox::from_id_salt("target_lang")
                        .selected_text(tgt_display)
                        .show_ui(ui, |ui| {
                            for (name, code) in TARGET_LANGS {
                                ui.selectable_value(&mut self.config.target_lang, code.to_string(), *name);
                            }
                        });
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // --- Mode ---
                    ui.label("Mode:");
                    let mode_label = match self.config.mode {
                        TranslationMode::Oneshot => "One-shot",
                        TranslationMode::Live => "Live",
                    };
                    egui::ComboBox::from_id_salt("mode")
                        .selected_text(mode_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.mode, TranslationMode::Oneshot, "One-shot");
                            ui.selectable_value(&mut self.config.mode, TranslationMode::Live, "Live");
                        });
                    ui.end_row();

                    // --- Live interval ---
                    ui.label("Live interval (ms):");
                    ui.add(egui::DragValue::new(&mut self.config.live_interval_ms).range(200..=10000).speed(50));
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);

            // --- OCR Languages ---
            ui.label(egui::RichText::new("OCR Languages (Tesseract):").strong());
            ui.add_space(4.0);

            if self.available_ocr_langs.is_empty() {
                ui.label("No Tesseract languages found. Install tesseract-ocr.");
            } else {
                let cols = 4;
                egui::Grid::new("ocr_langs_grid")
                    .num_columns(cols)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for (i, lang) in self.available_ocr_langs.iter().enumerate() {
                            ui.checkbox(&mut self.selected_ocr_langs[i], lang);
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });

                let ocr_str = self.build_ocr_lang_string();
                if ocr_str.is_empty() {
                    ui.colored_label(egui::Color32::RED, "Select at least one OCR language!");
                } else {
                    ui.label(format!("Selected: {}", ocr_str));
                    self.config.ocr_lang = ocr_str;
                }
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("💾  Save").clicked() {
                    match self.config.save() {
                        Ok(_) => {
                            self.saved = true;
                            log::info!("Settings saved");
                        }
                        Err(e) => log::error!("Failed to save settings: {}", e),
                    }
                }
                if ui.button("✕  Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if self.saved {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ Saved! Restart eyeclipse to apply.");
                }
            });
        });
    }
}

/// Open the settings GUI window. Blocks until closed.
pub fn show_settings(config: &AppConfig) -> Result<()> {
    let available = detect_tesseract_langs();
    log::info!("Detected Tesseract languages: {:?}", available);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Eyeclipse Settings")
            .with_inner_size(egui::vec2(500.0, 550.0))
            .with_min_inner_size(egui::vec2(400.0, 400.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Eyeclipse Settings",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(SettingsApp::new(config.clone(), available)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Settings window error: {}", e))
}
