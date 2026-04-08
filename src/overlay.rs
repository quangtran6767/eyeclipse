use anyhow::Result;
use eframe::egui;
use std::sync::{Arc, Mutex};

pub struct OverlayResult {
    pub source_text: String,
    pub translated_text: String,
    pub region_x: i32,
    pub region_y: i32,
    pub region_width: u32,
    #[allow(dead_code)]
    pub region_height: u32,
}

struct OverlayApp {
    result: OverlayResult,
    should_close: bool,
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Close on Escape
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.should_close = true;
        }

        // Check for click outside (focus lost)
        if ctx.input(|i| i.viewport().close_requested()) {
            self.should_close = true;
        }

        if self.should_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 230)).inner_margin(12.0).rounding(8.0))
            .show(ctx, |ui| {
                ui.style_mut().visuals.override_text_color = Some(egui::Color32::WHITE);

                ui.heading("Eyeclipse");
                ui.separator();

                // Source text
                ui.label(egui::RichText::new("Source:").color(egui::Color32::LIGHT_GRAY).size(11.0));
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .id_salt("source_scroll")
                    .show(ui, |ui| {
                        ui.label(&self.result.source_text);
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Translated text
                ui.label(egui::RichText::new("Translation:").color(egui::Color32::LIGHT_GREEN).size(11.0));
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .id_salt("translation_scroll")
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&self.result.translated_text)
                                .size(14.0)
                                .color(egui::Color32::WHITE),
                        );
                    });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.button("📋 Copy").clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&self.result.translated_text);
                        }
                    }
                    if ui.button("✕ Close").clicked() {
                        self.should_close = true;
                    }
                });
            });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // Transparent
    }
}

pub fn show_overlay(result: OverlayResult) -> Result<()> {
    // Position overlay near the selected region (below and to the right)
    let pos_x = result.region_x as f32 + result.region_width as f32 + 10.0;
    let pos_y = result.region_y as f32;

    // Ensure it stays on screen — clamp to reasonable values
    let pos_x = pos_x.min(1600.0);
    let pos_y = pos_y.max(10.0);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_transparent(true)
            .with_position(egui::pos2(pos_x, pos_y))
            .with_inner_size(egui::vec2(400.0, 350.0))
            .with_min_inner_size(egui::vec2(250.0, 150.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Eyeclipse Overlay",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(OverlayApp {
                result,
                should_close: false,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Overlay error: {}", e))
}

/// Shared state for updating the overlay from the live mode thread.
pub struct LiveOverlayState {
    pub source_text: Arc<Mutex<String>>,
    pub translated_text: Arc<Mutex<String>>,
    pub should_close: Arc<Mutex<bool>>,
}

pub struct LiveOverlayApp {
    state: LiveOverlayState,
}

impl eframe::App for LiveOverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            *self.state.should_close.lock().unwrap() = true;
        }

        if *self.state.should_close.lock().unwrap() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let source = self.state.source_text.lock().unwrap().clone();
        let translated = self.state.translated_text.lock().unwrap().clone();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 230)).inner_margin(12.0).rounding(8.0))
            .show(ctx, |ui| {
                ui.style_mut().visuals.override_text_color = Some(egui::Color32::WHITE);

                ui.heading("Eyeclipse (Live)");
                ui.separator();

                ui.label(egui::RichText::new("Source:").color(egui::Color32::LIGHT_GRAY).size(11.0));
                egui::ScrollArea::vertical()
                    .max_height(100.0)
                    .id_salt("source_scroll")
                    .show(ui, |ui| {
                        ui.label(&source);
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.label(egui::RichText::new("Translation:").color(egui::Color32::LIGHT_GREEN).size(11.0));
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .id_salt("translation_scroll")
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&translated)
                                .size(14.0)
                                .color(egui::Color32::WHITE),
                        );
                    });

                ui.add_space(8.0);
                if ui.button("⏹ Stop Live").clicked() {
                    *self.state.should_close.lock().unwrap() = true;
                }
            });

        // Request repaint to check for updates
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

pub fn show_live_overlay(
    region_x: i32,
    region_y: i32,
    region_width: u32,
) -> Result<LiveOverlayState> {
    let state = LiveOverlayState {
        source_text: Arc::new(Mutex::new("Monitoring...".to_string())),
        translated_text: Arc::new(Mutex::new(String::new())),
        should_close: Arc::new(Mutex::new(false)),
    };

    let app_state = LiveOverlayState {
        source_text: Arc::clone(&state.source_text),
        translated_text: Arc::clone(&state.translated_text),
        should_close: Arc::clone(&state.should_close),
    };

    let pos_x = (region_x as f32 + region_width as f32 + 10.0).min(1600.0);
    let pos_y = (region_y as f32).max(10.0);

    std::thread::spawn(move || {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_always_on_top()
                .with_transparent(true)
                .with_position(egui::pos2(pos_x, pos_y))
                .with_inner_size(egui::vec2(400.0, 350.0))
                .with_min_inner_size(egui::vec2(250.0, 150.0)),
            ..Default::default()
        };

        let _ = eframe::run_native(
            "Eyeclipse Live",
            options,
            Box::new(move |_cc| {
                Ok(Box::new(LiveOverlayApp { state: app_state }))
            }),
        );
    });

    Ok(state)
}
