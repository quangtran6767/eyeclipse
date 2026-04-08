use anyhow::Result;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture;

/// Load a system font that supports Vietnamese and CJK characters.
fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Try loading Noto Sans from common system paths
    let font_paths = [
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    ];

    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "system_font".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            // Insert at the front so it's preferred
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "system_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("system_font".to_owned());
            break;
        }
    }

    ctx.set_fonts(fonts);
}

/// Allow dragging the window by its background (no title bar).
fn enable_drag(ctx: &egui::Context) {
    // If the user is pressing on empty space (not a widget), start a native drag.
    let dominated_by_widget = ctx.input(|i| i.pointer.any_click()) && ctx.is_using_pointer();
    if !dominated_by_widget && ctx.input(|i| i.pointer.any_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// Compute overlay position: below the selection if room, otherwise above.
fn compute_overlay_position(
    region_x: i32,
    region_y: i32,
    _region_width: u32,
    region_height: u32,
    overlay_height: f32,
    gap: f32,
) -> (f32, f32) {
    let pos_x = (region_x as f32).max(0.0);

    // Get monitor height to decide below vs above
    let monitor_height = capture::get_monitor_dimensions(region_x, region_y)
        .map(|(_, h)| h as f32)
        .unwrap_or(1080.0);

    let below_y = region_y as f32 + region_height as f32 + gap;
    let pos_y = if below_y + overlay_height <= monitor_height {
        below_y
    } else {
        // Not enough room below — place above
        (region_y as f32 - overlay_height - gap).max(0.0)
    };

    (pos_x, pos_y)
}

pub struct OverlayResult {
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

        if ctx.input(|i| i.viewport().close_requested()) {
            self.should_close = true;
        }

        if self.should_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Allow dragging by clicking on window background
        enable_drag(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 230)).inner_margin(12.0).rounding(8.0))
            .show(ctx, |ui| {
                ui.style_mut().visuals.override_text_color = Some(egui::Color32::WHITE);

                // Drag handle
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⠿").color(egui::Color32::GRAY).size(10.0));
                    ui.label(egui::RichText::new("Eyeclipse").color(egui::Color32::GRAY).size(10.0));
                });
                ui.separator();

                // Translated text
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .id_salt("translation_scroll")
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&self.result.translated_text)
                                .size(15.0)
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
    let overlay_height = 250.0_f32;
    let gap = 5.0_f32;
    let overlay_width = (result.region_width as f32).max(200.0);
    let (pos_x, pos_y) = compute_overlay_position(
        result.region_x,
        result.region_y,
        result.region_width,
        result.region_height,
        overlay_height,
        gap,
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_transparent(true)
            .with_position(egui::pos2(pos_x, pos_y))
            .with_inner_size(egui::vec2(overlay_width, overlay_height))
            .with_min_inner_size(egui::vec2(200.0, 100.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Eyeclipse Overlay",
        options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            Ok(Box::new(OverlayApp {
                result,
                should_close: false,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Overlay error: {}", e))
}

struct LiveOverlayApp {
    translated_text: Arc<Mutex<String>>,
    stop_signal: Arc<AtomicBool>,
}

impl eframe::App for LiveOverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // External stop (hotkey pressed again)
        if self.stop_signal.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.stop_signal.store(true, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Allow dragging by clicking on window background
        enable_drag(ctx);

        let translated = self.translated_text.lock().unwrap().clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 230))
                    .inner_margin(12.0)
                    .rounding(8.0),
            )
            .show(ctx, |ui| {
                ui.style_mut().visuals.override_text_color = Some(egui::Color32::WHITE);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⠿").color(egui::Color32::GRAY).size(10.0));
                    ui.label(
                        egui::RichText::new("⏺ Live Translation")
                            .color(egui::Color32::LIGHT_GREEN)
                            .size(11.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⏹ Stop").clicked() {
                            self.stop_signal.store(true, Ordering::SeqCst);
                        }
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .id_salt("live_scroll")
                    .show(ui, |ui| {
                        if translated.is_empty() {
                            ui.label(
                                egui::RichText::new("Monitoring region...")
                                    .italics()
                                    .color(egui::Color32::GRAY),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(&translated)
                                    .size(15.0)
                                    .color(egui::Color32::WHITE),
                            );
                        }
                    });
            });

        // Poll for updates
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// Run the live overlay on the current (main) thread. Blocks until closed.
pub fn run_live_overlay(
    translated_text: Arc<Mutex<String>>,
    stop_signal: Arc<AtomicBool>,
    region_x: i32,
    region_y: i32,
    region_width: u32,
    region_height: u32,
) -> Result<()> {
    let overlay_height = 300.0_f32;
    let gap = 5.0_f32;
    let overlay_width = (region_width as f32).max(200.0);
    let (pos_x, pos_y) = compute_overlay_position(
        region_x,
        region_y,
        region_width,
        region_height,
        overlay_height,
        gap,
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_transparent(true)
            .with_position(egui::pos2(pos_x, pos_y))
            .with_inner_size(egui::vec2(overlay_width, overlay_height))
            .with_min_inner_size(egui::vec2(200.0, 100.0)),
        ..Default::default()
    };

    eframe::run_native(
        "Eyeclipse Live",
        options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            Ok(Box::new(LiveOverlayApp {
                translated_text,
                stop_signal,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Live overlay error: {}", e))
}
