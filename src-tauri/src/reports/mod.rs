use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::db::now_iso;
use crate::oauth::drive;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ExportReceiptResult {
    pub json_path: String,
    pub html_path: String,
    pub exported_at: String,
}

#[derive(Debug, Serialize)]
struct ReceiptDocument {
    exported_at: String,
    app: String,
    google_account: Option<String>,
    vault_path: Option<String>,
    storage_quota: Option<drive::StorageQuota>,
    recoverable_bytes: i64,
    recoverable_count: i64,
    google_drive_only_count: i64,
    google_drive_only_bytes: i64,
    phone_only_count: i64,
    total_files_checked: i64,
    proof_samples: Vec<ProofLine>,
    activity_summary: String,
}

#[derive(Debug, Serialize)]
struct ProofLine {
    filename: String,
    size_bytes: i64,
    copy_on_pc: Option<String>,
}

pub fn export_audit_receipt(state: &AppState) -> Result<ExportReceiptResult, String> {
    let vault = crate::config::AppConfig::load(&state.data_dir)
        .vault_path
        .or_else(|| {
            let db = state.db.lock().ok()?;
            db.with_conn(|conn| crate::db::get_setting(conn, "vault_path"))
                .ok()
                .flatten()
        });

    let export_dir = if let Some(ref v) = vault {
        PathBuf::from(v).join("_deduper").join("receipts")
    } else {
        state.data_dir.join("receipts")
    };
    fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;

    let ts = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let json_path = export_dir.join(format!("deduper-report-{ts}.json"));
    let html_path = export_dir.join(format!("deduper-report-{ts}.html"));

    let quota = drive::get_storage_quota(state).ok();
    let email = drive::get_auth_status(state).ok().and_then(|s| s.email);

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let doc = db
        .with_conn(|conn| {
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
            let google_drive_only_bytes: i64 = conn.query_row(
                "SELECT COALESCE(SUM(f.size_bytes),0) FROM files f
                 JOIN sources s ON f.source_id = s.id
                 WHERE s.source_type = 'google_drive' AND f.confidence = 'unique'",
                [],
                |row| row.get(0),
            )?;
            let google_drive_only_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM files f
                 JOIN sources s ON f.source_id = s.id
                 WHERE s.source_type = 'google_drive' AND f.confidence = 'unique'",
                [],
                |row| row.get(0),
            )?;
            let phone_only_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM files f
                 JOIN sources s ON f.source_id = s.id
                 WHERE s.source_type IN ('android_mtp','phone_import') AND f.confidence = 'unique'",
                [],
                |row| row.get(0),
            )?;
            let total_files_checked: i64 =
                conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

            let mut stmt = conn.prepare(
                "SELECT rc.filename, rc.size_bytes, l.relative_path, s.source_type, s.config_json
                 FROM recovery_candidates rc
                 JOIN files l ON rc.matched_local_file_id = l.id
                 JOIN sources s ON l.source_id = s.id
                 WHERE rc.verified_safe = 1
                 ORDER BY rc.size_bytes DESC LIMIT 25",
            )?;
            let proof_samples: Vec<ProofLine> = stmt
                .query_map([], |row| {
                    let relative_path: String = row.get(2)?;
                    let source_type: String = row.get(3)?;
                    let config_str: String = row.get(4)?;
                    let config: serde_json::Value =
                        serde_json::from_str(&config_str).unwrap_or(serde_json::json!({}));
                    let copy_on_pc = crate::scanner::vault::resolve_local_path(
                        &source_type,
                        &config,
                        &relative_path,
                    )
                    .map(|p| p.to_string_lossy().into_owned());
                    Ok(ProofLine {
                        filename: row.get(0)?,
                        size_bytes: row.get(1)?,
                        copy_on_pc,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            let activity_summary = format!(
                "Checked {total_files_checked} files. {recoverable_count} Google Drive files ({:.1} GB) are already saved on your PC or phone.",
                recoverable_bytes as f64 / 1024f64.powi(3)
            );

            Ok(ReceiptDocument {
                exported_at: now_iso(),
                app: "Deduper".into(),
                google_account: email.clone(),
                vault_path: vault.clone(),
                storage_quota: quota.clone(),
                recoverable_bytes,
                recoverable_count,
                google_drive_only_count,
                google_drive_only_bytes,
                phone_only_count,
                total_files_checked,
                proof_samples,
                activity_summary,
            })
        })
        .map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    fs::write(&json_path, &json).map_err(|e| e.to_string())?;

    let html = render_html(&doc);
    fs::write(&html_path, html).map_err(|e| e.to_string())?;

    crate::audit::log_action(
        state,
        "receipt_exported",
        &serde_json::json!({
            "json": json_path.to_string_lossy(),
            "html": html_path.to_string_lossy(),
        }),
        false,
    )?;

    Ok(ExportReceiptResult {
        json_path: json_path.to_string_lossy().into_owned(),
        html_path: html_path.to_string_lossy().into_owned(),
        exported_at: doc.exported_at,
    })
}

fn render_html(doc: &ReceiptDocument) -> String {
    let quota_line = doc
        .storage_quota
        .as_ref()
        .map(|q| {
            format!(
                "<p>Google storage: <strong>{}</strong> used of <strong>{}</strong> ({} free)</p>",
                q.usage_display, q.limit_display, q.free_display
            )
        })
        .unwrap_or_default();

    let mut proof_rows = String::new();
    for p in &doc.proof_samples {
        proof_rows.push_str(&format!(
            "<tr><td>{}</td><td>{:.1} MB</td><td>{}</td></tr>",
            html_escape(&p.filename),
            p.size_bytes as f64 / 1024f64.powi(2),
            html_escape(p.copy_on_pc.as_deref().unwrap_or("—"))
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>Deduper Report</title>
<style>
body {{ font-family: Segoe UI, sans-serif; max-width: 800px; margin: 2rem auto; padding: 0 1rem; color: #222; }}
h1 {{ color: #1a5a9e; }} table {{ border-collapse: collapse; width: 100%; margin-top: 1rem; }}
th, td {{ border: 1px solid #ccc; padding: 8px; text-align: left; font-size: 14px; }}
th {{ background: #f0f4f8; }} .summary {{ background: #fff8ee; padding: 1rem; border-radius: 8px; }}
</style></head><body>
<h1>Deduper Activity Report</h1>
<p>Generated: {}</p>
<p>Google account: {}</p>
<p>PC photo folder: {}</p>
{quota_line}
<div class="summary"><p>{}</p></div>
<h2>Sample proof (Google Drive files already on your PC)</h2>
<table><thead><tr><th>File</th><th>Size</th><th>Already saved at</th></tr></thead>
<tbody>{}</tbody></table>
<p style="margin-top:2rem;font-size:12px;color:#666;">Deduper never deletes files automatically. Open this report in a browser and use Print → Save as PDF if you need a PDF copy.</p>
</body></html>"#,
        doc.exported_at,
        doc.google_account.as_deref().unwrap_or("Not connected"),
        doc.vault_path.as_deref().unwrap_or("Not set"),
        html_escape(&doc.activity_summary),
        proof_rows
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
