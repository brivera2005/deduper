pub mod audit;
pub mod commands;
pub mod config;
pub mod db;
pub mod hash;
pub mod oauth;
pub mod reports;
pub mod scanner;
pub mod state;
pub mod watcher;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent,
};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data directory");
            std::fs::create_dir_all(&data_dir).ok();

            let db_path = data_dir.join("deduper.db");
            let database = db::Database::open(&db_path).expect("failed to open database");
            let state = Arc::new(AppState::new(database, data_dir));

            app.manage(state);

            let show_i = MenuItem::with_id(app, "show", "Show Deduper", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Deduper — safe media consolidation")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // USB device watcher — tray notification when Android MTP connects
            watcher::spawn_usb_watcher(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::list_sources,
            commands::add_local_source,
            commands::add_phone_import_folder,
            commands::start_scan,
            commands::get_scan_status,
            commands::cancel_scan,
            commands::get_drive_auth_status,
            commands::connect_google_drive,
            commands::disconnect_google_drive,
            commands::get_recovery_report,
            commands::copy_uniques_to_vault,
            commands::get_audit_recommendations,
            commands::start_full_audit,
            commands::get_full_audit_status,
            commands::cancel_full_audit,
            commands::get_audit_log,
            commands::get_setup_status,
            commands::complete_setup_step,
            commands::get_watcher_status,
            commands::get_google_oauth_config,
            commands::save_google_oauth_config,
            commands::get_wizard_status,
            commands::complete_wizard,
            commands::reset_wizard,
            commands::set_vault_path,
            commands::pick_vault_folder,
            commands::get_vault_path,
            commands::detect_android_devices,
            commands::connect_android_device,
            commands::get_android_status,
            commands::get_google_storage_quota,
            commands::connect_google_cleanup,
            commands::move_duplicates_to_trash,
            commands::export_audit_receipt,
            commands::open_receipt_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Deduper")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                // Minimize to tray instead of quitting
                api.prevent_exit();
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
        });
}
