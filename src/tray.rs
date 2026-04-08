use anyhow::Result;
use muda::{Menu, MenuItem, PredefinedMenuItem};
use std::sync::mpsc;
use tray_icon::{Icon, TrayIconBuilder};

pub enum TrayEvent {
    ToggleMode,
    Capture,
    OpenConfig,
    Quit,
}

#[cfg(target_os = "linux")]
pub fn start_tray(tx: mpsc::Sender<TrayEvent>) -> Result<()> {
    std::thread::spawn(move || {
        gtk::init().expect("Failed to init GTK for tray");

        let menu = Menu::new();

        let capture_item = MenuItem::new("Capture Region (Super+Shift+S)", true, None);
        let toggle_item = MenuItem::new("Toggle Live Mode", true, None);
        let config_item = MenuItem::new("Settings", true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        menu.append(&capture_item).unwrap();
        menu.append(&toggle_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&config_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&quit_item).unwrap();

        let capture_id = capture_item.id().clone();
        let toggle_id = toggle_item.id().clone();
        let config_id = config_item.id().clone();
        let quit_id = quit_item.id().clone();

        // Create a simple 16x16 icon (blue circle on transparent background)
        let icon_rgba = create_default_icon();
        let icon = Icon::from_rgba(icon_rgba, 16, 16).expect("Failed to create tray icon");

        let _tray = TrayIconBuilder::new()
            .with_tooltip("Eyeclipse — Screen Translator")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .expect("Failed to build tray icon");

        let menu_rx = muda::MenuEvent::receiver();

        // GTK main loop
        let tx_clone = tx.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if let Ok(event) = menu_rx.try_recv() {
                if event.id() == &capture_id {
                    let _ = tx_clone.send(TrayEvent::Capture);
                } else if event.id() == &toggle_id {
                    let _ = tx_clone.send(TrayEvent::ToggleMode);
                } else if event.id() == &config_id {
                    let _ = tx_clone.send(TrayEvent::OpenConfig);
                } else if event.id() == &quit_id {
                    let _ = tx_clone.send(TrayEvent::Quit);
                    gtk::main_quit();
                    return glib::ControlFlow::Break;
                }
            }
            glib::ControlFlow::Continue
        });

        gtk::main();
    });

    Ok(())
}

#[cfg(target_os = "macos")]
pub fn start_tray(tx: mpsc::Sender<TrayEvent>) -> Result<()> {
    std::thread::spawn(move || {
        let menu = Menu::new();

        let capture_item = MenuItem::new("Capture Region (Cmd+Shift+S)", true, None);
        let toggle_item = MenuItem::new("Toggle Live Mode", true, None);
        let config_item = MenuItem::new("Settings", true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        menu.append(&capture_item).unwrap();
        menu.append(&toggle_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&config_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&quit_item).unwrap();

        let capture_id = capture_item.id().clone();
        let toggle_id = toggle_item.id().clone();
        let config_id = config_item.id().clone();
        let quit_id = quit_item.id().clone();

        let icon_rgba = create_default_icon();
        let icon = Icon::from_rgba(icon_rgba, 16, 16).expect("Failed to create tray icon");

        let _tray = TrayIconBuilder::new()
            .with_tooltip("Eyeclipse — Screen Translator")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .expect("Failed to build tray icon");

        let menu_rx = muda::MenuEvent::receiver();

        // macOS: tray-icon uses native Cocoa — just poll the menu event receiver
        loop {
            match menu_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(event) => {
                    if event.id() == &capture_id {
                        let _ = tx.send(TrayEvent::Capture);
                    } else if event.id() == &toggle_id {
                        let _ = tx.send(TrayEvent::ToggleMode);
                    } else if event.id() == &config_id {
                        let _ = tx.send(TrayEvent::OpenConfig);
                    } else if event.id() == &quit_id {
                        let _ = tx.send(TrayEvent::Quit);
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok(())
}

fn create_default_icon() -> Vec<u8> {
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let radius = 6.0f32;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;

            if dist <= radius {
                // Blue-ish color: #4a9eff
                rgba[idx] = 0x4a;     // R
                rgba[idx + 1] = 0x9e; // G
                rgba[idx + 2] = 0xff; // B
                rgba[idx + 3] = 0xff; // A
            }
        }
    }

    rgba
}


