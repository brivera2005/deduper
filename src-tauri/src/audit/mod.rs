use rusqlite::params;
use serde::Serialize;
use uuid::Uuid;

use crate::db::now_iso;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub action: String,
    pub details: serde_json::Value,
    pub dry_run: bool,
    pub created_at: String,
}

pub fn log_action(
    state: &AppState,
    action: &str,
    details: &serde_json::Value,
    dry_run: bool,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO audit_log (id, action, details_json, dry_run, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                Uuid::new_v4().to_string(),
                action,
                details.to_string(),
                dry_run as i64,
                now_iso(),
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn list_entries(state: &AppState, limit: i64) -> Result<Vec<AuditEntry>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, action, details_json, dry_run, created_at
             FROM audit_log ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let details_str: String = row.get(2)?;
                Ok(AuditEntry {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    details: serde_json::from_str(&details_str).unwrap_or(serde_json::json!({})),
                    dry_run: row.get::<_, i64>(3)? != 0,
                    created_at: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    })
    .map_err(|e| e.to_string())
}
