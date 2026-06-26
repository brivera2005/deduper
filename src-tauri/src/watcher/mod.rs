use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

static KNOWN_DEVICES: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Poll for Android MTP devices and notify when a new phone connects.
pub fn spawn_usb_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        // Seed known devices without notifying on first poll.
        if let Ok(mut known) = KNOWN_DEVICES.lock() {
            let devices = crate::scanner::mtp::MtpScanner::detect_devices();
            let current: HashSet<String> = devices
                .iter()
                .map(|d| format!("{}|{}", d.name, d.storage_path))
                .collect();
            *known = Some(current);
        }

        loop {
            std::thread::sleep(Duration::from_secs(30));
            let devices = crate::scanner::mtp::MtpScanner::detect_devices();
            let current: HashSet<String> = devices
                .iter()
                .map(|d| format!("{}|{}", d.name, d.storage_path))
                .collect();

            let mut known = KNOWN_DEVICES.lock().unwrap();
            if known.is_none() {
                *known = Some(current.clone());
                continue;
            }

            let prev = known.as_ref().unwrap();
            for key in current.difference(prev) {
                let device_name = key.split('|').next().unwrap_or("Phone");
                let _ = app
                    .notification()
                    .builder()
                    .title("Phone detected")
                    .body(format!(
                        "{device_name} connected — open Deduper to scan your photos."
                    ))
                    .show();
            }
            *known = Some(current);
        }
    });
}

#[derive(serde::Serialize)]
pub struct WatcherStatus {
    pub active: bool,
    pub phase: &'static str,
    pub connected_devices: Vec<String>,
    pub immich_integration: &'static str,
}

pub fn get_status() -> WatcherStatus {
    let devices = crate::scanner::mtp::MtpScanner::detect_devices();
    WatcherStatus {
        active: true,
        phase: "USB polling every 30s — tray notification on new device",
        connected_devices: devices
            .into_iter()
            .map(|d| format!("{} ({})", d.name, d.storage_name))
            .collect(),
        immich_integration: "planned for v2",
    }
}
