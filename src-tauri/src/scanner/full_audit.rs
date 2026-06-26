use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::audit;
use crate::db::now_iso;
use crate::scanner::mtp;
use crate::state::AppState;

use super::engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAuditProgress {
    pub job_id: String,
    pub status: String,
    pub phase: String,
    pub message: String,
    pub sources_total: i64,
    pub sources_done: i64,
    pub current_source_name: Option<String>,
    pub current_file: Option<String>,
    pub files_processed: i64,
    pub files_total: i64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAuditOptions {
    pub include_google_drive: bool,
    pub include_google_photos: bool,
    pub include_gmail: bool,
    pub include_this_pc: bool,
    pub include_phone: bool,
}

impl Default for FullAuditOptions {
    fn default() -> Self {
        Self {
            include_google_drive: true,
            include_google_photos: true,
            include_gmail: true,
            include_this_pc: true,
            include_phone: true,
        }
    }
}

fn source_display_name(source_type: &str, name: &str) -> String {
    match source_type {
        "google_drive" => format!("Google Drive — {name}"),
        "google_photos" => format!("Google Photos — {name}"),
        "gmail_attachments" => format!("Gmail attachments — {name}"),
        "android_mtp" => format!("Your phone — {name}"),
        "phone_import" => format!("Phone backup folder — {name}"),
        "local" => format!("This PC — {name}"),
        _ => name.to_string(),
    }
}

fn phase_label(source_type: &str) -> &'static str {
    match source_type {
        "google_drive" => "Checking Google Drive",
        "google_photos" => "Checking Google Photos",
        "gmail_attachments" => "Checking Gmail attachments",
        "android_mtp" => "Checking your phone",
        "phone_import" => "Checking phone backup folder",
        "local" => "Checking this PC",
        _ => "Checking files",
    }
}

pub fn run_full_audit(
    state: Arc<AppState>,
    job_id: String,
    options: FullAuditOptions,
) -> Result<(), String> {
    state.reset_full_audit_cancel();

    let sources: Vec<(String, String, String)> = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source_type, name FROM sources ORDER BY
                 CASE source_type
                   WHEN 'google_drive' THEN 1
                   WHEN 'google_photos' THEN 2
                   WHEN 'gmail_attachments' THEN 3
                   WHEN 'local' THEN 4
                   WHEN 'phone_import' THEN 5
                   WHEN 'android_mtp' THEN 6
                   ELSE 7
                 END, created_at",
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
                .filter(|(_, t, _)| match t.as_str() {
                    "google_drive" => options.include_google_drive,
                    "google_photos" => options.include_google_photos,
                    "gmail_attachments" => options.include_gmail,
                    "local" => options.include_this_pc,
                    "android_mtp" | "phone_import" => options.include_phone,
                    _ => true,
                })
                .collect();
            Ok(rows)
        })
        .map_err(|e| e.to_string())?
    };

    let total = sources.len() as i64;
    update_audit_progress(
        &state,
        &job_id,
        "running",
        "starting",
        "Getting ready to check all your photos and videos…",
        0,
        total,
        None,
        None,
        0,
        0,
        None,
    )?;

    if sources.is_empty() {
        update_audit_progress(
            &state,
            &job_id,
            "failed",
            "starting",
            "Nothing to check yet — connect Google Drive and pick a folder on this PC in Setup.",
            0,
            0,
            None,
            None,
            0,
            0,
            Some("No sources connected"),
        )?;
        return Err("Connect Google Drive and your PC folder first (use Setup).".into());
    }

    for (idx, (source_id, source_type, name)) in sources.iter().enumerate() {
        if state.is_full_audit_cancelled() {
            update_audit_progress(
                &state,
                &job_id,
                "cancelled",
                "cancelled",
                "Check stopped.",
                idx as i64,
                total,
                None,
                None,
                0,
                0,
                None,
            )?;
            return Ok(());
        }

        let display = source_display_name(source_type, name);
        let phase = phase_label(source_type);

        update_audit_progress(
            &state,
            &job_id,
            "running",
            source_type,
            &format!("{phase}…"),
            idx as i64,
            total,
            Some(display.clone()),
            None,
            0,
            0,
            None,
        )?;

        let scan_job_id = Uuid::new_v4().to_string();
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
                    params![scan_job_id, source_id, now_iso()],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        }

        state.reset_scan_cancel();
        match engine::run_scan(state.clone(), source_id.clone(), scan_job_id) {
            Ok(()) => {}
            Err(e) if source_type == "android_mtp" || source_type == "google_photos" || source_type == "gmail_attachments" => {
                engine::update_job_failed(&state, &scan_job_id, &e)?;
                let db = state.db.lock().map_err(|e| e.to_string())?;
                db.with_conn(|conn| {
                    conn.execute(
                        "UPDATE sources SET status = 'idle' WHERE id = ?1",
                        params![source_id],
                    )?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
                audit::log_action(
                    &state,
                    "phone_scan_skipped",
                    &serde_json::json!({ "reason": e, "source_id": source_id }),
                    true,
                )?;
                continue;
            }
            Err(e) => {
                engine::update_job_failed(&state, &scan_job_id, &e)?;
                update_audit_progress(
                    &state,
                    &job_id,
                    "failed",
                    source_type,
                    &format!("Problem while checking {display}"),
                    idx as i64,
                    total,
                    Some(display),
                    None,
                    0,
                    0,
                    Some(&e),
                )?;
                return Err(e);
            }
        }
    }

    update_audit_progress(
        &state,
        &job_id,
        "running",
        "analyzing",
        "Finding duplicates and calculating how much Google Drive space you can free up…",
        total,
        total,
        None,
        None,
        0,
        0,
        None,
    )?;

    engine::recompute_duplicates(&state)?;
    engine::build_recovery_candidates(&state)?;

    update_audit_progress(
        &state,
        &job_id,
        "completed",
        "done",
        "All done! Scroll down to see your results and proof.",
        total,
        total,
        None,
        None,
        0,
        0,
        None,
    )?;

    audit::log_action(
        &state,
        "full_audit_completed",
        &serde_json::json!({ "job_id": job_id, "sources": total }),
        true,
    )?;

    Ok(())
}

pub fn update_audit_progress(
    state: &AppState,
    job_id: &str,
    status: &str,
    phase: &str,
    message: &str,
    sources_done: i64,
    sources_total: i64,
    current_source_name: Option<String>,
    current_file: Option<String>,
    files_processed: i64,
    files_total: i64,
    error_message: Option<&str>,
) -> Result<(), String> {
    let mut guard = state.active_full_audit.lock().map_err(|e| e.to_string())?;
    *guard = Some(FullAuditProgress {
        job_id: job_id.to_string(),
        status: status.into(),
        phase: phase.into(),
        message: message.into(),
        sources_total,
        sources_done,
        current_source_name,
        current_file,
        files_processed,
        files_total,
        error_message: error_message.map(String::from),
    });
    Ok(())
}
