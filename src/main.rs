use anyhow::Result;
use eyeclipse::config::{AppConfig, TranslationMode};
use eyeclipse::{capture, hotkey, live, overlay, selector, settings, translate};
#[cfg(feature = "ocr")]
use eyeclipse::ocr;
#[cfg(feature = "tray")]
use eyeclipse::tray;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

/// Global flag: is a capture/overlay currently in progress?
static BUSY: AtomicBool = AtomicBool::new(false);

/// Global flag: is live mode currently running?
static LIVE_RUNNING: AtomicBool = AtomicBool::new(false);

/// Signal to stop the live monitor thread.
static LIVE_STOP: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(
            "info,winit=warn,tracing=warn,eframe=warn,wgpu=warn,naga=warn,zbus=warn,calloop=warn,smithay=warn,glutin=warn,accesskit=warn"
        )
    ).init();

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
                // If live mode is running, stop it
                if LIVE_RUNNING.load(Ordering::SeqCst) {
                    log::info!("Stopping live mode");
                    LIVE_STOP.store(true, Ordering::SeqCst);
                    continue;
                }
                // Ignore if already busy (dedup rapid hotkey presses)
                if BUSY.swap(true, Ordering::SeqCst) {
                    log::debug!("Ignoring duplicate hotkey while busy");
                    continue;
                }
                log::info!("Capture triggered");
                handle_capture(&config, &rt);
                BUSY.store(false, Ordering::SeqCst);
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
            Ok(AppEvent::OpenConfig) => {
                log::info!("Opening settings GUI");
                if let Err(e) = settings::show_settings(&config) {
                    log::error!("Settings window error: {}", e);
                }
                // Reload config after settings window closes
                match AppConfig::load() {
                    Ok(new_config) => {
                        config = new_config;
                        log::info!("Config reloaded after settings change");
                    }
                    Err(e) => log::error!("Failed to reload config: {}", e),
                }
            }
            Ok(AppEvent::Quit) => {
                log::info!("Quit requested");
                LIVE_STOP.store(true, Ordering::SeqCst);
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
    OpenConfig,
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
                tray::TrayEvent::OpenConfig => AppEvent::OpenConfig,
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
        TranslationMode::Live => handle_live(config, region),
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

fn handle_live(config: &AppConfig, region: selector::Region) {
    // Reset stop signal
    LIVE_STOP.store(false, Ordering::SeqCst);
    LIVE_RUNNING.store(true, Ordering::SeqCst);

    let ocr_lang = config.ocr_lang.clone();
    let source_lang = config.source_lang.clone();
    let target_lang = config.target_lang.clone();
    let interval_ms = config.live_interval_ms;
    let backend = translate::create_backend(config);

    // Send initial notification
    notify("Live mode started", "Press Super+Shift+S again to stop");

    // Spawn the entire live monitor in a background thread — returns immediately
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) = live::start_live_monitor(
                region,
                &ocr_lang,
                &source_lang,
                &target_lang,
                backend.as_ref(),
                interval_ms,
                &LIVE_STOP,
            )
            .await
            {
                log::error!("Live monitor error: {}", e);
            }
        });

        LIVE_RUNNING.store(false, Ordering::SeqCst);
        notify("Live mode stopped", "");
        log::info!("Live mode ended");
    });
}

/// Show a desktop notification via notify-send.
fn notify(summary: &str, body: &str) {
    let mut cmd = std::process::Command::new("notify-send");
    cmd.arg("-a").arg("Eyeclipse")
        .arg("-u").arg("normal")
        .arg("-h").arg("string:x-canonical-private-synchronous:eyeclipse-live")
        .arg(summary);
    if !body.is_empty() {
        cmd.arg(body);
    }
    let _ = cmd.spawn();
}
