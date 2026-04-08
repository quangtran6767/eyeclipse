use anyhow::{Context, Result};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use std::sync::mpsc;

pub enum HotkeyAction {
    CaptureRegion,
}

pub struct HotkeyListener {
    _manager: GlobalHotKeyManager,
    _hotkey_id: u32,
}

impl HotkeyListener {
    pub fn new(tx: mpsc::Sender<HotkeyAction>) -> Result<Self> {
        let manager = GlobalHotKeyManager::new().context("Failed to create hotkey manager")?;

        // Super+Shift+S
        let hotkey = HotKey::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            Code::KeyS,
        );
        let hotkey_id = hotkey.id();

        manager.register(hotkey).context("Failed to register hotkey Super+Shift+S")?;

        let receiver = GlobalHotKeyEvent::receiver();

        std::thread::spawn(move || {
            loop {
                if let Ok(event) = receiver.recv() {
                    if event.id() == hotkey_id {
                        log::info!("Hotkey triggered");
                        if tx.send(HotkeyAction::CaptureRegion).is_err() {
                            log::error!("Hotkey channel closed");
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            _manager: manager,
            _hotkey_id: hotkey_id,
        })
    }
}
