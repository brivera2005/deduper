use std::path::PathBuf;
use std::sync::Arc;

use rayon::prelude::*;
use rusqlite::params;
use uuid::Uuid;

use crate::db::now_iso;
use crate::hash;
use crate::state::AppState;

use super::{
    drive::DriveScanner,
    gmail::GmailScanner,
    local::{LocalScanner, PhoneImportScanner},
    mtp::MtpScanner,
    photos::PhotosScanner,
    ScannedItem, SourceScanner, SourceType,
};

const HASH_BATCH: usize = 64;

pub fn build_scanner(
    source_type: &SourceType,
    config: &serde_json::Value,
    access_token: Option<String>,
) -> Result<Box<dyn SourceScanner>, String> {
    match source_type {
        SourceType::Local => {
            let path = config
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing local path")?;
            Ok(Box::new(LocalScanner::new(PathBuf::from(path))))
        }
        SourceType::PhoneImport => {
            let path = config
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing import path")?;
            Ok(Box::new(PhoneImportScanner::new(PathBuf::from(path))))
        }
        SourceType::GoogleDrive => {
            let token = access_token.ok_or("Google account not connected")?;
            Ok(Box::new(DriveScanner::new(token)))
        }
        SourceType::GooglePhotos => {
            let token = access_token.ok_or("Google account not connected")?;
            Ok(Box::new(PhotosScanner::new(token)))
        }
        SourceType::GmailAttachments => {
            let token = access_token.ok_or("Google account not connected")?;
            Ok(Box::new(GmailScanner::new(token)))
        }
        SourceType::AndroidMtp => {
            let name = config
                .get("device_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Android Device")
                .to_string();
            let storage_path = config
                .get("storage_path")
                .and_then(|v| v.as_str())
                .ok_or("missing phone storage path — reconnect your phone")?
                .to_string();
            crate::scanner::mtp::validate_device_connected(&storage_path)?;
            Ok(Box::new(MtpScanner::new(name, storage_path)))
        }
    }
}

pub fn run_scan(
    state: Arc<AppState>,
    source_id: String,
    job_id: String,
) -> Result<(), String> {
    let (source_type_str, config) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT source_type, config_json FROM sources WHERE id = ?1",
            )?;
            let row = stmt.query_row(params![source_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(row)
        })
        .map_err(|e| e.to_string())?
    };

    let source_type = SourceType::from_str(&source_type_str)
        .ok_or_else(|| format!("unknown source type: {source_type_str}"))?;
    let config: serde_json::Value =
        serde_json::from_str(&config).map_err(|e| e.to_string())?;

    let access_token = match source_type {
        SourceType::GoogleDrive | SourceType::GooglePhotos | SourceType::GmailAttachments => {
            Some(crate::oauth::drive::get_valid_access_token(&state)?)
        }
        _ => None,
    };

    let scanner = build_scanner(&source_type, &config, access_token)?;
    let items = scanner.list_files()?;

    update_job_totals(&state, &job_id, items.len() as i64)?;

    let checkpoint = load_checkpoint(&state, &job_id)?;
    let start_index = checkpoint.unwrap_or(0) as usize;
    let is_cloud = matches!(
        source_type,
        SourceType::GoogleDrive | SourceType::GooglePhotos | SourceType::GmailAttachments
    );

    for batch_start in (start_index..items.len()).step_by(HASH_BATCH) {
        if state.is_scan_cancelled() {
            save_checkpoint(&state, &job_id, batch_start as i64)?;
            update_job_status(&state, &job_id, "cancelled", None)?;
            return Ok(());
        }

        let batch_end = (batch_start + HASH_BATCH).min(items.len());
        let batch = &items[batch_start..batch_end];

        if !is_cloud {
            let hashed: Vec<(usize, Option<(String, Option<String>)>)> = batch
                .par_iter()
                .enumerate()
                .map(|(i, item)| {
                    let global_idx = batch_start + i;
                    let hash_pair = if let Some(cached) =
                        load_cached_hash(&state, &source_id, item)?
                    {
                        Some(cached)
                    } else {
                        resolve_content_hash(&*scanner, item, false)?
                    };
                    Ok((global_idx, hash_pair))
                })
                .collect::<Result<Vec<_>, String>>()?;

            for (idx, hash_pair) in hashed {
                let item = &items[idx];
                update_progress(&state, &job_id, &source_id, idx as i64 + 1, &item.filename)?;
                let file_id = Uuid::new_v4().to_string();
                insert_file(&state, &file_id, &source_id, item, &hash_pair)?;
                increment_hashed(&state, &job_id)?;
            }
        } else {
            for (i, item) in batch.iter().enumerate() {
                let idx = batch_start + i;
                update_progress(&state, &job_id, &source_id, idx as i64 + 1, &item.filename)?;
                let file_id = Uuid::new_v4().to_string();
                let hash_pair = resolve_content_hash(&*scanner, item, true)?;
                insert_file(&state, &file_id, &source_id, item, &hash_pair)?;
                increment_hashed(&state, &job_id)?;
            }
        }

        save_checkpoint(&state, &job_id, batch_end as i64)?;
    }

    recompute_duplicates(&state)?;
    if source_type == SourceType::GoogleDrive {
        build_recovery_candidates(&state)?;
    }

    update_source_stats(&state, &source_id)?;
    update_job_status(&state, &job_id, "completed", None)?;
    clear_checkpoint(&state, &job_id)?;

    {
        let mut active = state.active_scan.lock().map_err(|e| e.to_string())?;
        if let Some(ref mut p) = *active {
            if p.job_id == job_id {
                p.status = "completed".into();
            }
        }
    }

    crate::audit::log_action(
        &state,
        "scan_completed",
        &serde_json::json!({ "source_id": source_id, "job_id": job_id }),
        true,
    )?;

    Ok(())
}

/// Returns (content_hash, md5_checksum column value).
fn resolve_content_hash(
    scanner: &dyn SourceScanner,
    item: &ScannedItem,
    is_cloud: bool,
) -> Result<Option<(String, Option<String>)>, String> {
    if let Some(ref preset) = item.content_hash {
        return Ok(Some((preset.clone(), item.md5_checksum.clone())));
    }

    if is_cloud {
        if let Some(md5) = &item.md5_checksum {
            let normalized = hash::normalize_md5(md5);
            return Ok(Some((
                hash::content_identity_from_md5(&normalized),
                Some(normalized),
            )));
        }
        return Ok(None);
    }

    let path = scanner.read_file_for_hash(item)?;
    let fp = hash::fingerprint_file(&path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    Ok(Some((fp.content_hash, Some(fp.md5_hex))))
}

fn load_cached_hash(
    state: &AppState,
    source_id: &str,
    item: &ScannedItem,
) -> Result<Option<(String, Option<String>)>, String> {
    let modified = item.modified_at.map(|d| d.to_rfc3339());
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT content_hash, md5_checksum FROM files
             WHERE source_id = ?1 AND relative_path = ?2 AND size_bytes = ?3
             AND content_hash IS NOT NULL
             AND (modified_at IS ?4 OR ?4 IS NULL)",
        )?;
        let result = stmt.query_row(
            params![
                source_id,
                item.relative_path,
                item.size_bytes as i64,
                modified
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        );
        match result {
            Ok(pair) => Ok(Some(pair)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
    .map_err(|e| e.to_string())
}

fn update_job_totals(state: &AppState, job_id: &str, total: i64) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE scan_jobs SET total_files = ?1, status = 'running', started_at = COALESCE(started_at, ?2) WHERE id = ?3",
            params![total, now_iso(), job_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn update_progress(
    state: &AppState,
    job_id: &str,
    source_id: &str,
    processed: i64,
    current_file: &str,
) -> Result<(), String> {
    {
        let mut active = state.active_scan.lock().map_err(|e| e.to_string())?;
        *active = Some(super::ScanProgress {
            job_id: job_id.to_string(),
            source_id: source_id.to_string(),
            status: "running".into(),
            total_files: 0,
            processed_files: processed,
            hashed_files: processed,
            current_file: Some(current_file.to_string()),
            error_message: None,
        });
    }

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE scan_jobs SET processed_files = ?1 WHERE id = ?2",
            params![processed, job_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn increment_hashed(state: &AppState, job_id: &str) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE scan_jobs SET hashed_files = hashed_files + 1 WHERE id = ?1",
            params![job_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn insert_file(
    state: &AppState,
    file_id: &str,
    source_id: &str,
    item: &ScannedItem,
    hash_pair: &Option<(String, Option<String>)>,
) -> Result<(), String> {
    let (content_hash, md5_checksum) = match hash_pair {
        Some((h, m)) => (Some(h.clone()), m.clone()),
        None => (None, None),
    };

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO files (id, source_id, relative_path, filename, size_bytes, mime_type,
             modified_at, content_hash, md5_checksum, drive_file_id, confidence, scanned_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'unknown',?11)
             ON CONFLICT(source_id, relative_path) DO UPDATE SET
               size_bytes = excluded.size_bytes,
               content_hash = excluded.content_hash,
               md5_checksum = excluded.md5_checksum,
               scanned_at = excluded.scanned_at",
            params![
                file_id,
                source_id,
                item.relative_path,
                item.filename,
                item.size_bytes as i64,
                item.mime_type,
                item.modified_at.map(|d| d.to_rfc3339()),
                content_hash,
                md5_checksum,
                item.drive_file_id,
                now_iso(),
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn recompute_duplicates(state: &AppState) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute("DELETE FROM duplicate_groups", [])?;
        conn.execute(
            "UPDATE files SET confidence = 'unknown', duplicate_group_id = NULL WHERE content_hash IS NOT NULL",
            [],
        )?;

        let mut stmt = conn.prepare(
            "SELECT content_hash, COUNT(*), SUM(size_bytes), MIN(id)
             FROM files WHERE content_hash IS NOT NULL AND content_hash != ''
             GROUP BY content_hash HAVING COUNT(*) > 1",
        )?;

        let groups: Vec<(String, i64, i64, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (hash, count, total_size, primary_id) in groups {
            let group_id = Uuid::new_v4().to_string();
            let confidence = if hash.starts_with("likely:") {
                "likely_duplicate"
            } else {
                "verified_duplicate"
            };
            conn.execute(
                "INSERT INTO duplicate_groups (id, content_hash, file_count, total_size_bytes, primary_file_id, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![group_id, hash, count, total_size, primary_id, now_iso()],
            )?;
            conn.execute(
                "UPDATE files SET confidence = ?1, duplicate_group_id = ?2
                 WHERE content_hash = ?3",
                params![confidence, group_id, hash],
            )?;
        }

        conn.execute(
            "UPDATE files SET confidence = 'unique'
             WHERE content_hash IS NOT NULL AND content_hash != ''
             AND duplicate_group_id IS NULL",
            [],
        )?;

        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn build_recovery_candidates(state: &AppState) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute("DELETE FROM recovery_candidates", [])?;

        conn.execute(
            "INSERT INTO recovery_candidates (id, drive_file_id, filename, size_bytes, content_hash, matched_local_file_id, verified_safe, created_at)
             SELECT
               lower(hex(randomblob(16))),
               d.drive_file_id,
               d.filename,
               d.size_bytes,
               d.content_hash,
               l.id,
               1,
               ?1
             FROM files d
             JOIN files l ON d.content_hash = l.content_hash AND d.id != l.id
             JOIN sources sd ON d.source_id = sd.id AND sd.source_type = 'google_drive'
             JOIN sources sl ON l.source_id = sl.id AND sl.source_type IN ('local', 'phone_import', 'android_mtp')
             WHERE d.confidence = 'verified_duplicate'
             AND d.drive_file_id IS NOT NULL
             AND d.content_hash IS NOT NULL",
            params![now_iso()],
        )?;

        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn update_source_stats(state: &AppState, source_id: &str) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE sources SET
               file_count = (SELECT COUNT(*) FROM files WHERE source_id = ?1),
               total_bytes = (SELECT COALESCE(SUM(size_bytes),0) FROM files WHERE source_id = ?1),
               last_scan_at = ?2,
               status = 'idle'
             WHERE id = ?1",
            params![source_id, now_iso()],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn update_job_status(
    state: &AppState,
    job_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE scan_jobs SET status = ?1, error_message = ?2,
             completed_at = CASE WHEN ?1 IN ('completed','failed','cancelled') THEN ?3 ELSE completed_at END
             WHERE id = ?4",
            params![status, error, now_iso(), job_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn save_checkpoint(state: &AppState, job_id: &str, index: i64) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE scan_jobs SET checkpoint_path = ?1 WHERE id = ?2",
            params![index.to_string(), job_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn load_checkpoint(state: &AppState, job_id: &str) -> Result<Option<i64>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT checkpoint_path FROM scan_jobs WHERE id = ?1")?;
        let result = stmt.query_row(params![job_id], |row| row.get::<_, Option<String>>(0));
        match result {
            Ok(Some(s)) => Ok(s.parse().ok()),
            Ok(None) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
    .map_err(|e| e.to_string())
}

fn clear_checkpoint(state: &AppState, job_id: &str) -> Result<(), String> {
    save_checkpoint(state, job_id, 0)
}

pub fn update_job_failed(state: &AppState, job_id: &str, error: &str) -> Result<(), String> {
    update_job_status(state, job_id, "failed", Some(error))
}
