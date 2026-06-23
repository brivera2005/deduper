use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use rusqlite::params;
use serde::Serialize;
use tauri::Manager;
use uuid::Uuid;

use crate::audit;
use crate::config::{self, AppConfig};
use crate::db::{get_setting, now_iso, set_setting};
use crate::oauth::drive;
use crate::scanner::{engine, mtp, SourceRecord, SourceType};
use crate::state::AppState;
use crate::watcher;

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub recoverable_bytes: i64,
    pub recoverable_count: i64,
    pub total_files: i64,
    pub duplicate_groups: i64,
    pub sources_connected: i64,
    pub vault_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecoveryReport {
    pub recoverable_bytes: i64,
    pub recoverable_count: i64,
    pub sample_files: Vec<RecoverySample>,
    pub safety_note: String,
}

#[derive(Debug, Serialize)]
pub struct RecoverySample {
    pub filename: String,
    pub size_bytes: i64,
    pub drive_file_id: String,
}

#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub welcome_done: bool,
    pub local_added: bool,
    pub drive_connected: bool,
    pub android_connected: bool,
    pub first_scan_done: bool,
    pub vault_set: bool,
    pub wizard_completed: bool,
    pub wizard_skipped: bool,
}

#[derive(Debug, Serialize)]
pub struct WizardStatus {
    pub completed: bool,
    pub skipped: bool,
    pub completed_at: Option<String>,
    pub vault_path: Option<String>,
    pub google_configured: bool,
    pub drive_connected: bool,
    pub drive_email: Option<String>,
    pub android_connected: bool,
    pub android_device_name: Option<String>,
    pub local_source_count: i64,
    pub first_scan_done: bool,
}

#[derive(Debug, Serialize)]
pub struct CopyResult {
    pub copied_count: i64,
    pub skipped_count: i64,
    pub dry_run: bool,
    pub destination: String,
}

#[tauri::command]
pub fn get_dashboard(state: tauri::State<'_, Arc<AppState>>) -> Result<DashboardStats, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let recoverable_bytes: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(size_bytes),0) FROM recovery_candidates WHERE verified_safe = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let recoverable_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM recovery_candidates WHERE verified_safe = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let total_files: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .unwrap_or(0);
        let duplicate_groups: i64 = conn
            .query_row("SELECT COUNT(*) FROM duplicate_groups", [], |row| row.get(0))
            .unwrap_or(0);
        let sources_connected: i64 = conn
            .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
            .unwrap_or(0);
        let vault_path = get_setting(conn, "vault_path")?;
        let cfg_vault = crate::config::AppConfig::load(&state.data_dir).vault_path;

        Ok(DashboardStats {
            recoverable_bytes,
            recoverable_count,
            total_files,
            duplicate_groups,
            sources_connected,
            vault_path: cfg_vault.or(vault_path),
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sources(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<SourceRecord>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, source_type, name, config_json, status, last_scan_at, file_count, total_bytes
             FROM sources ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let type_str: String = row.get(1)?;
                Ok(SourceRecord {
                    id: row.get(0)?,
                    source_type: SourceType::from_str(&type_str).unwrap_or(SourceType::Local),
                    name: row.get(2)?,
                    config: serde_json::from_str(&row.get::<_, String>(3)?)
                        .unwrap_or(serde_json::json!({})),
                    status: row.get(4)?,
                    last_scan_at: row.get(5)?,
                    file_count: row.get(6)?,
                    total_bytes: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_local_source(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
    name: Option<String>,
) -> Result<SourceRecord, String> {
    let path_buf = PathBuf::from(&path);
    crate::scanner::local::validate_folder(&path_buf)?;

    let id = Uuid::new_v4().to_string();
    let display_name = name.unwrap_or_else(|| {
        path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Local Folder")
            .to_string()
    });
    let config = serde_json::json!({ "path": path });

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sources (id, source_type, name, config_json, status, created_at)
             VALUES (?1, 'local', ?2, ?3, 'idle', ?4)",
            params![id, display_name, config.to_string(), now_iso()],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;

    audit::log_action(
        &state,
        "source_added",
        &serde_json::json!({ "type": "local", "path": path }),
        true,
    )?;

    Ok(SourceRecord {
        id,
        source_type: SourceType::Local,
        name: display_name,
        config,
        status: "idle".into(),
        last_scan_at: None,
        file_count: 0,
        total_bytes: 0,
    })
}

#[tauri::command]
pub fn add_phone_import_folder(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<SourceRecord, String> {
    let path_buf = PathBuf::from(&path);
    crate::scanner::local::validate_folder(&path_buf)?;

    let id = Uuid::new_v4().to_string();
    let config = serde_json::json!({ "path": path });

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sources (id, source_type, name, config_json, status, created_at)
             VALUES (?1, 'phone_import', ?2, ?3, 'idle', ?4)",
            params![id, "Phone Import", config.to_string(), now_iso()],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;

    audit::log_action(
        &state,
        "source_added",
        &serde_json::json!({ "type": "phone_import", "path": path }),
        true,
    )?;

    Ok(SourceRecord {
        id,
        source_type: SourceType::PhoneImport,
        name: "Phone Import".into(),
        config,
        status: "idle".into(),
        last_scan_at: None,
        file_count: 0,
        total_bytes: 0,
    })
}

#[tauri::command]
pub fn start_scan(
    state: tauri::State<'_, Arc<AppState>>,
    source_id: String,
) -> Result<String, String> {
    let job_id = Uuid::new_v4().to_string();

    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE sources SET status = 'scanning' WHERE id = ?1",
                params![source_id],
            )?;
            conn.execute(
                "INSERT INTO scan_jobs (id, source_id, status, started_at)
                 VALUES (?1, ?2, 'pending', ?3)",
                params![job_id, source_id, now_iso()],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    }

    state.reset_scan_cancel();

    let state_clone = Arc::clone(&state);
    let source_id_for_thread = source_id.clone();
    let job_id_clone = job_id.clone();

    thread::spawn(move || {
        if let Err(e) =
            engine::run_scan(state_clone.clone(), source_id_for_thread.clone(), job_id_clone.clone())
        {
            let _ = update_job_failed(&state_clone, &job_id_clone, &e);
            let mut active = state_clone.active_scan.lock().unwrap();
            if let Some(ref mut p) = *active {
                p.status = "failed".into();
                p.error_message = Some(e);
            }
        }
        let db = state_clone.db.lock().unwrap();
        let _ = db.with_conn(|conn| {
            conn.execute(
                "UPDATE sources SET status = 'idle' WHERE id = ?1",
                params![source_id_for_thread],
            )?;
            Ok(())
        });
    });

    audit::log_action(
        &state,
        "scan_started",
        &serde_json::json!({ "source_id": source_id, "job_id": job_id }),
        true,
    )?;

    Ok(job_id)
}

fn update_job_failed(state: &AppState, job_id: &str, error: &str) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE scan_jobs SET status = 'failed', error_message = ?1, completed_at = ?2 WHERE id = ?3",
            params![error, now_iso(), job_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_scan_status(
    state: tauri::State<'_, Arc<AppState>>,
    job_id: Option<String>,
) -> Result<Option<crate::scanner::ScanProgress>, String> {
    if let Some(active) = state.active_scan.lock().map_err(|e| e.to_string())?.clone() {
        if job_id.as_ref().map(|j| j == &active.job_id).unwrap_or(true) {
            return Ok(Some(active));
        }
    }

    if let Some(jid) = job_id {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        return db
            .with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, source_id, status, total_files, processed_files, hashed_files, error_message
                     FROM scan_jobs WHERE id = ?1",
                )?;
                let result = stmt.query_row(params![jid], |row| {
                    Ok(crate::scanner::ScanProgress {
                        job_id: row.get(0)?,
                        source_id: row.get(1)?,
                        status: row.get(2)?,
                        total_files: row.get(3)?,
                        processed_files: row.get(4)?,
                        hashed_files: row.get(5)?,
                        current_file: None,
                        error_message: row.get(6)?,
                    })
                });
                match result {
                    Ok(p) => Ok(Some(p)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e.into()),
                }
            })
            .map_err(|e| e.to_string());
    }

    Ok(None)
}

#[tauri::command]
pub fn cancel_scan(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state.request_scan_cancel();
    audit::log_action(&state, "scan_cancelled", &serde_json::json!({}), true)
}

#[tauri::command]
pub fn get_drive_auth_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<drive::DriveAuthStatus, String> {
    drive::get_auth_status(&state)
}

#[tauri::command]
pub fn connect_google_drive(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<drive::DriveAuthStatus, String> {
    drive::connect_drive_blocking(Arc::clone(&state))
}

#[tauri::command]
pub fn disconnect_google_drive(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    drive::disconnect_drive(&state)
}

#[tauri::command]
pub fn get_recovery_report(state: tauri::State<'_, Arc<AppState>>) -> Result<RecoveryReport, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let recoverable_bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size_bytes),0) FROM recovery_candidates WHERE verified_safe = 1",
            [],
            |row| row.get(0),
        )?;
        let recoverable_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM recovery_candidates WHERE verified_safe = 1",
            [],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT filename, size_bytes, drive_file_id FROM recovery_candidates
             WHERE verified_safe = 1 ORDER BY size_bytes DESC LIMIT 10",
        )?;
        let sample_files: Vec<RecoverySample> = stmt
            .query_map([], |row| {
                Ok(RecoverySample {
                    filename: row.get(0)?,
                    size_bytes: row.get(1)?,
                    drive_file_id: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(RecoveryReport {
            recoverable_bytes,
            recoverable_count,
            sample_files,
            safety_note: "These Drive files are verified duplicates of files on your PC or phone import. \
                          Deduper never auto-deletes. Review the report, then optionally move to Drive Trash with explicit confirmation."
                .into(),
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_uniques_to_vault(
    state: tauri::State<'_, Arc<AppState>>,
    destination: String,
    dry_run: bool,
) -> Result<CopyResult, String> {
    let dest = PathBuf::from(&destination);
    if !dry_run {
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    }

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let files: Vec<(String, String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT f.id, f.filename, s.config_json
                 FROM files f
                 JOIN sources s ON f.source_id = s.id
                 WHERE f.confidence = 'unique'
                 AND s.source_type IN ('local', 'phone_import', 'google_drive')",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
        .map_err(|e| e.to_string())?;

    let mut copied = 0i64;
    let mut skipped = 0i64;

    for (_id, filename, config_str) in files {
        let config: serde_json::Value =
            serde_json::from_str(&config_str).unwrap_or(serde_json::json!({}));
        if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
            let src = PathBuf::from(path).join(&filename);
            if !src.exists() {
                skipped += 1;
                continue;
            }
            let dst = dest.join(&filename);
            if dst.exists() {
                skipped += 1;
                continue;
            }
            if !dry_run {
                fs::copy(&src, &dst).map_err(|e| e.to_string())?;
            }
            copied += 1;
        } else {
            skipped += 1;
        }
    }

    if !dry_run {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.with_conn(|conn| set_setting(conn, "vault_path", &destination))
            .map_err(|e| e.to_string())?;
    }

    audit::log_action(
        &state,
        "copy_uniques_to_vault",
        &serde_json::json!({
            "destination": destination,
            "copied": copied,
            "skipped": skipped,
        }),
        dry_run,
    )?;

    Ok(CopyResult {
        copied_count: copied,
        skipped_count: skipped,
        dry_run,
        destination,
    })
}

#[tauri::command]
pub fn get_audit_log(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<i64>,
) -> Result<Vec<audit::AuditEntry>, String> {
    audit::list_entries(&state, limit.unwrap_or(50))
}

#[tauri::command]
pub fn get_setup_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SetupStatus, String> {
    let cfg = AppConfig::load(&state.data_dir);
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let welcome_done = cfg.wizard_done();
        let local_added: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sources WHERE source_type IN ('local', 'phone_import')",
            [],
            |row| row.get(0),
        )?;
        let android_connected: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sources WHERE source_type = 'android_mtp'",
            [],
            |row| row.get(0),
        )?;
        let drive_connected: i64 = conn.query_row(
            "SELECT COUNT(*) FROM oauth_tokens WHERE provider = 'google_drive'",
            [],
            |row| row.get(0),
        )?;
        let first_scan_done: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scan_jobs WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )?;
        let vault_set = cfg.vault_path.is_some()
            || get_setting(conn, "vault_path")?.is_some();

        Ok(SetupStatus {
            welcome_done,
            local_added: local_added > 0,
            drive_connected: drive_connected > 0,
            android_connected: android_connected > 0,
            first_scan_done: first_scan_done > 0,
            vault_set,
            wizard_completed: cfg.wizard_completed_at.is_some(),
            wizard_skipped: cfg.wizard_skipped,
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_setup_step(
    state: tauri::State<'_, Arc<AppState>>,
    step: String,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| set_setting(conn, &format!("setup_{step}"), "1"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_watcher_status() -> watcher::WatcherStatus {
    watcher::get_status()
}

#[tauri::command]
pub fn get_google_oauth_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<config::GoogleOAuthConfigStatus, String> {
    Ok(config::get_google_oauth_status(&state.data_dir))
}

#[tauri::command]
pub fn save_google_oauth_config(
    state: tauri::State<'_, Arc<AppState>>,
    client_id: String,
    client_secret: String,
) -> Result<config::GoogleOAuthConfigStatus, String> {
    config::save_google_credentials(&state.data_dir, client_id, client_secret)?;
    Ok(config::get_google_oauth_status(&state.data_dir))
}

#[tauri::command]
pub fn get_wizard_status(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<WizardStatus, String> {
    let cfg = AppConfig::load(&state.data_dir);
    let drive = drive::get_auth_status(&state)?;

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let android_connected: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sources WHERE source_type = 'android_mtp'",
            [],
            |row| row.get(0),
        )?;
        let local_source_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sources WHERE source_type IN ('local', 'phone_import')",
            [],
            |row| row.get(0),
        )?;
        let first_scan_done: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scan_jobs WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )?;

        let android_device_name = if android_connected > 0 {
            conn.query_row(
                "SELECT name FROM sources WHERE source_type = 'android_mtp' ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
        } else {
            None
        };

        let vault_path = cfg
            .vault_path
            .clone()
            .or_else(|| get_setting(conn, "vault_path").ok().flatten());

        Ok(WizardStatus {
            completed: cfg.wizard_done(),
            skipped: cfg.wizard_skipped,
            completed_at: cfg.wizard_completed_at.clone(),
            vault_path,
            google_configured: config::get_google_oauth_status(&state.data_dir).configured,
            drive_connected: drive.connected,
            drive_email: drive.email,
            android_connected: android_connected > 0,
            android_device_name,
            local_source_count,
            first_scan_done: first_scan_done > 0,
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_wizard(
    state: tauri::State<'_, Arc<AppState>>,
    skipped: bool,
) -> Result<(), String> {
    config::complete_wizard(&state.data_dir, skipped)?;
    audit::log_action(
        &state,
        if skipped {
            "wizard_skipped"
        } else {
            "wizard_completed"
        },
        &serde_json::json!({}),
        true,
    )
}

#[tauri::command]
pub fn reset_wizard(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    config::reset_wizard(&state.data_dir)
}

fn apply_vault_path(state: &AppState, path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    crate::scanner::local::validate_folder(&path_buf)?;
    config::set_vault_path(&state.data_dir, path.clone())?;

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| set_setting(conn, "vault_path", &path))
        .map_err(|e| e.to_string())?;

    audit::log_action(
        state,
        "vault_path_set",
        &serde_json::json!({ "path": path }),
        true,
    )
}

#[tauri::command]
pub fn set_vault_path(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<(), String> {
    apply_vault_path(&state, path)
}

/// Pick a vault folder via native dialog on a worker thread (avoids Windows webview deadlocks).
#[tauri::command]
pub async fn pick_vault_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let mut builder = app
        .dialog()
        .file()
        .set_title("Choose your vault folder");
    if let Some(window) = app.get_webview_window("main") {
        builder = builder.set_parent(&window);
    }

    let picked = tauri::async_runtime::spawn_blocking(move || builder.blocking_pick_folder())
        .await
        .map_err(|e| e.to_string())?;

    let Some(file_path) = picked else {
        return Ok(None);
    };

    let path = file_path
        .into_path()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();

    apply_vault_path(&state, path.clone())?;
    Ok(Some(path))
}

#[tauri::command]
pub fn get_vault_path(state: tauri::State<'_, Arc<AppState>>) -> Result<Option<String>, String> {
    let cfg = AppConfig::load(&state.data_dir);
    if let Some(p) = cfg.vault_path {
        return Ok(Some(p));
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| get_setting(conn, "vault_path"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn detect_android_devices() -> Result<Vec<mtp::MtpDeviceInfo>, String> {
    let devices = mtp::get_mtp_status();
    if devices.is_empty() {
        return Err(
            "No phone found. Plug in your Android phone via USB, unlock it, and choose \
             \"File transfer\" or \"Transfer files\" on the phone notification."
                .into(),
        );
    }
    Ok(devices)
}

#[tauri::command]
pub fn connect_android_device(
    state: tauri::State<'_, Arc<AppState>>,
    storage_path: String,
    device_name: Option<String>,
) -> Result<SourceRecord, String> {
    mtp::validate_device_connected(&storage_path)?;

    let devices = mtp::MtpScanner::detect_devices();
    let device = devices
        .iter()
        .find(|d| d.storage_path == storage_path)
        .ok_or("Phone not found — reconnect and try again")?;

    let display_name = device_name.unwrap_or_else(|| format!("{} ({})", device.name, device.storage_name));
    let config = serde_json::json!({
        "device_name": device.name,
        "storage_name": device.storage_name,
        "storage_path": storage_path,
    });

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let existing: Option<String> = db
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM sources WHERE source_type = 'android_mtp' AND config_json LIKE ?1 LIMIT 1",
            )?;
            let pattern = format!("%{}%", storage_path.replace('\\', "\\\\"));
            let result = stmt.query_row(params![pattern], |row| row.get(0));
            match result {
                Ok(id) => Ok(Some(id)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .map_err(|e| e.to_string())?;

    if let Some(id) = existing {
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE sources SET name = ?1, config_json = ?2, status = 'idle' WHERE id = ?3",
                params![display_name, config.to_string(), id],
            )?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;

        return Ok(SourceRecord {
            id,
            source_type: SourceType::AndroidMtp,
            name: display_name,
            config,
            status: "idle".into(),
            last_scan_at: None,
            file_count: 0,
            total_bytes: 0,
        });
    }

    let id = Uuid::new_v4().to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sources (id, source_type, name, config_json, status, created_at)
             VALUES (?1, 'android_mtp', ?2, ?3, 'idle', ?4)",
            params![id, display_name, config.to_string(), now_iso()],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;

    audit::log_action(
        &state,
        "android_connected",
        &serde_json::json!({ "device": device.name, "storage": device.storage_name }),
        true,
    )?;

    Ok(SourceRecord {
        id,
        source_type: SourceType::AndroidMtp,
        name: display_name,
        config,
        status: "idle".into(),
        last_scan_at: None,
        file_count: 0,
        total_bytes: 0,
    })
}

#[tauri::command]
pub fn get_android_status() -> Result<Vec<mtp::MtpDeviceInfo>, String> {
    Ok(mtp::get_mtp_status())
}
