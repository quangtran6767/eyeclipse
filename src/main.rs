mod capture;
mod config;
mod diff;
mod hotkey;
mod live;
#[cfg(feature = "ocr")]
mod ocr;
mod overlay;
mod selector;
mod translate;
#[cfg(feature = "tray")]
mod tray;

use anyhow::Result;
use config::{AppConfig, TranslationMode};
use std::sync::mpsc;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Eyeclipse starting...");

    // Load config
    let mut config = AppConfig::load()?;
    log::info!("Config loaded: {:?}", config.api_backend);

    // Check tesseract availability
    #[cfg(feature = "ocr")]
    if let Err(e) = ocr::check_tesseract_available(&config.ocr_lang) {
        log::error!("{}", e);
        eprintln!(
            "Error: Tesseract OCR is not available.\n\
             Install it with: sudo apt install tesseract-ocr tesseract-ocr-jpn tesseract-ocr-eng\n\
             Details: {}",
            e
        );
        std::process::exit(1);
    }

    // Warn if no API key
    if config.api_key.is_empty() {
        log::warn!(
            "No API key configured. Set it in {:?}",
            AppConfig::config_path().unwrap_or_default()
        );
        eprintln!(
            "Warning: No API key set. Translation will fail.\n\
             Edit {:?} and set your api_key.",
            AppConfig::config_path().unwrap_or_default()
        );
    }

    // Create channels for events
    let (hotkey_tx, event_rx) = mpsc::channel();

    // Start tray icon (GTK thread)
    #[cfg(feature = "tray")]
    {
        let tray_sender = hotkey_tx.clone();
        tray::start_tray(tray_event_adapter(tray_sender))?;
    }

    // Register global hotkey
    let _hotkey_listener = hotkey::HotkeyListener::new(hotkey_action_adapter(hotkey_tx))?;

    log::info!("Eyeclipse ready. Press Super+Shift+S to capture a region.");

    // Main event loop
    let rt = tokio::runtime::Runtime::new()?;

    loop {
        match event_rx.recv() {
            Ok(AppEvent::CaptureRegion) => {
                log::info!("Capture triggered");
                handle_capture(&config, &rt);
            }
            Ok(AppEvent::ToggleMode) => {
                config.mode = match config.mode {
                    TranslationMode::Oneshot => {
                        log::info!("Switched to Live mode");
                        TranslationMode::Live
                    }
                    TranslationMode::Live => {
                        log::info!("Switched to Oneshot mode");
                        TranslationMode::Oneshot
                    }
                };
                let _ = config.save();
            }
            Ok(AppEvent::Quit) => {
                log::info!("Quit requested");
                break;
            }
            Err(_) => {
                log::error!("Event channel closed");
                break;
            }
        }
    }

    Ok(())
}

enum AppEvent {
    CaptureRegion,
    ToggleMode,
    Quit,
}

fn hotkey_action_adapter(tx: mpsc::Sender<AppEvent>) -> mpsc::Sender<hotkey::HotkeyAction> {
    let (htx, hrx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(action) = hrx.recv() {
            match action {
                hotkey::HotkeyAction::CaptureRegion => {
                    let _ = tx.send(AppEvent::CaptureRegion);
                }
            }
        }
    });
    htx
}

#[cfg(feature = "tray")]
fn tray_event_adapter(tx: mpsc::Sender<AppEvent>) -> mpsc::Sender<tray::TrayEvent> {
    let (ttx, trx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(event) = trx.recv() {
            let app_event = match event {
                tray::TrayEvent::Capture => AppEvent::CaptureRegion,
                tray::TrayEvent::ToggleMode => AppEvent::ToggleMode,
                tray::TrayEvent::Quit => AppEvent::Quit,
            };
            let _ = tx.send(app_event);
        }
    });
    ttx
}

fn handle_capture(config: &AppConfig, rt: &tokio::runtime::Runtime) {
    // 1. Region selection
    let region = match selector::select_region() {
        Ok(Some(r)) => r,
        Ok(None) => {
            log::info!("Selection cancelled");
            return;
        }
        Err(e) => {
            log::error!("Selection failed: {}", e);
            return;
        }
    };

    log::info!(
        "Selected region: {}x{} at ({}, {})",
        region.width,
        region.height,
        region.x,
        region.y
    );

    match config.mode {
        TranslationMode::Oneshot => handle_oneshot(config, rt, region),
        TranslationMode::Live => handle_live(config, rt, region),
    }
}

fn handle_oneshot(config: &AppConfig, rt: &tokio::runtime::Runtime, region: selector::Region) {
    // 2. Capture the region
    let img = match capture::capture_region(region.x, region.y, region.width, region.height) {
        Ok(i) => i,
        Err(e) => {
            log::error!("Capture failed: {}", e);
            return;
        }
    };

    // 3. OCR
    #[cfg(feature = "ocr")]
    let text = match ocr::extract_text(&img, &config.ocr_lang) {
        Ok(t) => t,
        Err(e) => {
            log::error!("OCR failed: {}", e);
            return;
        }
    };
    #[cfg(not(feature = "ocr"))]
    let text = {
        log::error!("OCR feature not enabled. Build with --features ocr");
        return;
    };

    if text.is_empty() {
        log::warn!("No text detected in selected region");
        return;
    }

    log::info!("OCR result: {}", &text[..text.len().min(100)]);

    // 4. Translate
    let backend = translate::create_backend(config);
    let translated = match rt.block_on(backend.translate_dyn(
        text.clone(),
        config.source_lang.clone(),
        config.target_lang.clone(),
    )) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Translation failed: {}", e);
            format!("[Translation error: {}]", e)
        }
    };

    log::info!("Translation: {}", &translated[..translated.len().min(100)]);

    // 5. Show overlay
    let result = overlay::OverlayResult {
        source_text: text,
        translated_text: translated,
        region_x: region.x,
        region_y: region.y,
        region_width: region.width,
        region_height: region.height,
    };

    if let Err(e) = overlay::show_overlay(result) {
        log::error!("Overlay failed: {}", e);
    }
}

fn handle_live(config: &AppConfig, rt: &tokio::runtime::Runtime, region: selector::Region) {
    let overlay_state = match overlay::show_live_overlay(region.x, region.y, region.width) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create live overlay: {}", e);
            return;
        }
    };

    let backend = translate::create_backend(config);

    rt.block_on(async {
        if let Err(e) = live::start_live_monitor(
            region,
            &config.ocr_lang,
            &config.source_lang,
            &config.target_lang,
            backend.as_ref(),
            config.live_interval_ms,
            &overlay_state,
        )
        .await
        {
            log::error!("Live monitor error: {}", e);
        }
    });
}
