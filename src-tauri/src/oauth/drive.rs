use std::path::Path;
use std::sync::Arc;

use reqwest::blocking::Client;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, Server};

use crate::audit;
use crate::db::{now_iso, set_setting};
use crate::state::AppState;

const PROVIDER: &str = "google_drive";
pub const READONLY_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
pub const PHOTOS_SCOPE: &str = "https://www.googleapis.com/auth/photoslibrary.readonly";
pub const GMAIL_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const DRIVE_FULL_SCOPE: &str = "https://www.googleapis.com/auth/drive";

/// Scopes requested on normal Google sign-in (read-only everywhere).
pub const READ_SCOPES: &str =
    "https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/photoslibrary.readonly https://www.googleapis.com/auth/gmail.readonly";

/// Additional scope when user enables Google Drive cleanup (move to Trash).
pub const CLEANUP_SCOPES: &str = "https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/photoslibrary.readonly https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/drive";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DriveAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub scopes: Vec<String>,
    pub cleanup_enabled: bool,
    pub photos_enabled: bool,
    pub gmail_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageQuota {
    pub limit_bytes: i64,
    pub usage_bytes: i64,
    pub usage_in_drive_bytes: i64,
    pub usage_in_trash_bytes: i64,
    pub free_bytes: i64,
    pub usage_display: String,
    pub limit_display: String,
    pub free_display: String,
    pub percent_used: f64,
}

#[derive(Debug, Serialize)]
pub struct TrashResult {
    pub trashed_count: i64,
    pub skipped_count: i64,
    pub failed_count: i64,
    pub bytes_freed: i64,
    pub dry_run: bool,
    pub quota_after: Option<StorageQuota>,
}

fn oauth_port() -> u16 {
    std::env::var("DEDUPER_OAUTH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8888)
}

fn load_client_credentials(state: &AppState) -> Result<(String, String), String> {
    let cfg = crate::config::AppConfig::load(&state.data_dir);
    let id = cfg.google_client_id().ok_or(
        "Google sign-in is not available in this build. Reinstall from a release build, or add your own OAuth app under Advanced in setup.",
    )?;
    let secret = cfg.google_client_secret().ok_or(
        "Google sign-in is not available in this build. Reinstall from a release build, or add your own OAuth app under Advanced in setup.",
    )?;
    Ok((id, secret))
}

fn stored_scopes(state: &AppState) -> Result<Vec<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let scopes: String = db
        .with_conn(|conn| {
            conn.query_row(
                "SELECT scopes FROM oauth_tokens WHERE provider = ?1",
                params![PROVIDER],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .map_err(|_| "Google account not connected".to_string())?;
    Ok(scopes.split_whitespace().map(String::from).collect())
}

pub fn has_scope(state: &AppState, scope: &str) -> Result<bool, String> {
    Ok(stored_scopes(state)?.iter().any(|s| s == scope))
}

pub fn get_auth_status(state: &AppState) -> Result<DriveAuthStatus, String> {
    let token_data: Option<(String, String)> = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT access_token, scopes FROM oauth_tokens WHERE provider = ?1",
            )?;
            match stmt.query_row(params![PROVIDER], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                Ok(pair) => Ok(Some(pair)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .map_err(|e| e.to_string())?
    };

    match token_data {
        None => Ok(DriveAuthStatus {
            connected: false,
            email: None,
            scopes: vec![],
            cleanup_enabled: false,
            photos_enabled: false,
            gmail_enabled: false,
        }),
        Some((token, scopes)) => {
            let scope_list: Vec<String> = scopes.split_whitespace().map(String::from).collect();
            let email = fetch_token_email(&token).ok();
            Ok(DriveAuthStatus {
                connected: true,
                email,
                cleanup_enabled: scope_list.iter().any(|s| s == DRIVE_FULL_SCOPE),
                photos_enabled: scope_list.iter().any(|s| s == PHOTOS_SCOPE),
                gmail_enabled: scope_list.iter().any(|s| s == GMAIL_SCOPE),
                scopes: scope_list,
            })
        }
    }
}

fn fetch_token_email(access_token: &str) -> Result<String, String> {
    let client = Client::new();
    let resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err("failed to fetch user info".into());
    }

    #[derive(Deserialize)]
    struct UserInfo {
        email: String,
    }

    let info: UserInfo = resp.json().map_err(|e| e.to_string())?;
    Ok(info.email)
}

pub fn connect_drive_blocking(state: Arc<AppState>) -> Result<DriveAuthStatus, String> {
    run_oauth_flow(state, READ_SCOPES, "Deduper connected!")
}

pub fn connect_cleanup_blocking(state: Arc<AppState>) -> Result<DriveAuthStatus, String> {
    run_oauth_flow(
        state,
        CLEANUP_SCOPES,
        "Deduper cleanup enabled! You can move verified duplicates to Google Drive Trash.",
    )
}

fn run_oauth_flow(
    state: Arc<AppState>,
    scopes: &str,
    success_title: &str,
) -> Result<DriveAuthStatus, String> {
    let (client_id, client_secret) = load_client_credentials(&state)?;
    let port = oauth_port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");

    let (pkce_challenge, pkce_verifier) = oauth2::PkceCodeChallenge::new_random_sha256();

    let mut auth_url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|e| e.to_string())?;
    {
        let mut q = auth_url.query_pairs_mut();
        q.append_pair("client_id", &client_id);
        q.append_pair("redirect_uri", &redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("scope", scopes);
        q.append_pair("code_challenge", pkce_challenge.as_str());
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
    }

    open::that(auth_url.as_str()).map_err(|e| format!("failed to open browser: {e}"))?;

    let code = wait_for_callback(port, success_title)?;

    let client = Client::new();
    let token_resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", pkce_verifier.secret()),
        ])
        .send()
        .map_err(|e| format!("token request failed: {e}"))?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().unwrap_or_default();
        return Err(format!("token exchange failed: {body}"));
    }

    #[derive(Deserialize)]
    struct TokenResponseBody {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
        scope: Option<String>,
    }

    let tokens: TokenResponseBody = token_resp.json().map_err(|e| e.to_string())?;
    let expires_at = tokens.expires_in.map(|secs| {
        (chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339()
    });

    let scope_str = tokens.scope.as_deref().unwrap_or(scopes);
    let email = fetch_token_email(&tokens.access_token).ok();

    store_tokens(
        &state,
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
        expires_at.as_deref(),
        scope_str,
    )?;

    ensure_google_sources(&state)?;

    audit::log_action(
        &state,
        "drive_connected",
        &serde_json::json!({ "email": email, "scope": scope_str }),
        true,
    )?;

    get_auth_status(&state)
}

fn wait_for_callback(port: u16, success_title: &str) -> Result<String, String> {
    let server = Server::http(format!("127.0.0.1:{port}"))
        .map_err(|e| format!("failed to start OAuth callback server on port {port}: {e}"))?;

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        if let Some(query) = url.split('?').nth(1) {
            for pair in query.split('&') {
                if let Some(code) = pair.strip_prefix("code=") {
                    let code = decode_url_component(code);
                    let html = format!(
                        "<html><body style='font-family:sans-serif;text-align:center;padding:3rem'>
                        <h2>{success_title}</h2>
                        <p>You can close this tab and return to Deduper.</p></body></html>"
                    );
                    let response = Response::from_string(html)
                        .with_header(Header::from_bytes("Content-Type", "text/html").unwrap());
                    let _ = request.respond(response);
                    return Ok(code);
                }
                if pair.starts_with("error=") {
                    return Err(format!("OAuth denied: {pair}"));
                }
            }
        }
        let _ = request.respond(Response::from_string("Waiting for authorization..."));
    }

    Err("OAuth callback server stopped unexpectedly".into())
}

fn decode_url_component(s: &str) -> String {
    url::form_urlencoded::parse(s.as_bytes())
        .next()
        .map(|(k, _)| k.into_owned())
        .unwrap_or_else(|| s.to_string())
}

fn store_tokens(
    state: &AppState,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<&str>,
    scopes: &str,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO oauth_tokens (provider, access_token, refresh_token, expires_at, scopes)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(provider) DO UPDATE SET
               access_token = excluded.access_token,
               refresh_token = COALESCE(excluded.refresh_token, oauth_tokens.refresh_token),
               expires_at = excluded.expires_at,
               scopes = excluded.scopes",
            params![PROVIDER, access_token, refresh_token, expires_at, scopes],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn ensure_source(state: &AppState, source_type: &str, name: &str) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sources WHERE source_type = ?1",
            params![source_type],
            |row| row.get(0),
        )?;
        if exists == 0 {
            conn.execute(
                "INSERT INTO sources (id, source_type, name, config_json, status, created_at)
                 VALUES (?1, ?2, ?3, '{}', 'idle', ?4)",
                params![uuid::Uuid::new_v4().to_string(), source_type, name, now_iso()],
            )?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn ensure_google_sources(state: &AppState) -> Result<(), String> {
    ensure_source(state, "google_drive", "Google Drive")?;
    ensure_source(state, "google_photos", "Google Photos")?;
    ensure_source(state, "gmail_attachments", "Gmail large attachments")?;
    Ok(())
}

pub fn disconnect_drive(state: &AppState) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute("DELETE FROM oauth_tokens WHERE provider = ?1", params![PROVIDER])?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;

    audit::log_action(state, "drive_disconnected", &serde_json::json!({}), true)
}

pub fn get_valid_access_token(state: &AppState) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let (access_token, refresh_token, expires_at, scopes) = db
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT access_token, refresh_token, expires_at, scopes FROM oauth_tokens WHERE provider = ?1",
            )?;
            stmt.query_row(params![PROVIDER], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(Into::into)
        })
        .map_err(|_| "Google account not connected — connect Google Drive first".to_string())?;

    let needs_refresh = expires_at
        .as_ref()
        .and_then(|e| {
            chrono::DateTime::parse_from_rfc3339(e).ok().map(|exp| {
                chrono::Utc::now() + chrono::Duration::minutes(2)
                    > exp.with_timezone(&chrono::Utc)
            })
        })
        .unwrap_or(false);

    if !needs_refresh {
        return Ok(access_token);
    }

    let refresh = refresh_token.ok_or("Session expired — please connect Google again")?;
    refresh_access_token(state, &refresh, &scopes)
}

fn refresh_access_token(
    state: &AppState,
    refresh_token: &str,
    existing_scopes: &str,
) -> Result<String, String> {
    let (client_id, client_secret) = load_client_credentials(state)?;
    let client = Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err("Session expired — please connect Google again".into());
    }

    #[derive(Deserialize)]
    struct RefreshBody {
        access_token: String,
        expires_in: Option<i64>,
    }

    let body: RefreshBody = resp.json().map_err(|e| e.to_string())?;
    let expires_at = body.expires_in.map(|s| {
        (chrono::Utc::now() + chrono::Duration::seconds(s)).to_rfc3339()
    });

    store_tokens(
        state,
        &body.access_token,
        Some(refresh_token),
        expires_at.as_deref(),
        existing_scopes,
    )?;

    Ok(body.access_token)
}

pub fn get_storage_quota(state: &AppState) -> Result<StorageQuota, String> {
    let token = get_valid_access_token(state)?;
    let client = Client::new();
    let resp = client
        .get("https://www.googleapis.com/drive/v3/about?fields=storageQuota")
        .bearer_auth(&token)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err("Could not read your Google storage usage".into());
    }

    #[derive(Deserialize)]
    struct AboutResponse {
        #[serde(rename = "storageQuota")]
        storage_quota: StorageQuotaRaw,
    }

    #[derive(Deserialize)]
    struct StorageQuotaRaw {
        limit: Option<String>,
        usage: Option<String>,
        #[serde(rename = "usageInDrive")]
        usage_in_drive: Option<String>,
        #[serde(rename = "usageInDriveTrash")]
        usage_in_trash: Option<String>,
    }

    let about: AboutResponse = resp.json().map_err(|e| e.to_string())?;
    let q = about.storage_quota;

    let limit_bytes = q.limit.as_deref().unwrap_or("0").parse::<i64>().unwrap_or(0);
    let usage_bytes = q.usage.as_deref().unwrap_or("0").parse::<i64>().unwrap_or(0);
    let usage_in_drive_bytes = q
        .usage_in_drive
        .as_deref()
        .unwrap_or("0")
        .parse::<i64>()
        .unwrap_or(0);
    let usage_in_trash_bytes = q
        .usage_in_trash
        .as_deref()
        .unwrap_or("0")
        .parse::<i64>()
        .unwrap_or(0);
    let free_bytes = (limit_bytes - usage_bytes).max(0);
    let percent_used = if limit_bytes > 0 {
        (usage_bytes as f64 / limit_bytes as f64) * 100.0
    } else {
        0.0
    };

    let quota = StorageQuota {
        limit_bytes,
        usage_bytes,
        usage_in_drive_bytes,
        usage_in_trash_bytes,
        free_bytes,
        usage_display: format_bytes(usage_bytes),
        limit_display: format_bytes(limit_bytes),
        free_display: format_bytes(free_bytes),
        percent_used,
    };

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        set_setting(conn, "quota_snapshot_json", &serde_json::to_string(&quota).unwrap_or_default())
    })
    .map_err(|e| e.to_string())?;

    Ok(quota)
}

fn format_bytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 GB".into();
    }
    let gb = bytes as f64 / 1024f64.powi(3);
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1024f64.powi(2))
    }
}

pub fn move_recovery_candidates_to_trash(
    state: &AppState,
    dry_run: bool,
) -> Result<TrashResult, String> {
    if !has_scope(state, DRIVE_FULL_SCOPE)? {
        return Err(
            "Cleanup permission not granted. Click \"Enable Google Drive cleanup\" and sign in again."
                .into(),
        );
    }

    let token = get_valid_access_token(state)?;
    let quota_before = get_storage_quota(state).ok();

    let candidates: Vec<(String, i64)> = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT drive_file_id, size_bytes FROM recovery_candidates WHERE verified_safe = 1",
            )?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
        .map_err(|e| e.to_string())?
    };

    if candidates.is_empty() {
        return Err(
            "Nothing to move — run a full check first. Only files already saved on your PC or phone can be moved to Trash."
                .into(),
        );
    }

    let client = Client::new();
    let mut trashed = 0i64;
    let mut skipped = 0i64;
    let mut failed = 0i64;
    let mut bytes_freed = 0i64;

    for (file_id, size) in candidates {
        if dry_run {
            trashed += 1;
            bytes_freed += size;
            continue;
        }

        let url = format!("https://www.googleapis.com/drive/v3/files/{file_id}");
        let resp = client
            .patch(&url)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "trashed": true }))
            .send()
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            trashed += 1;
            bytes_freed += size;
        } else if resp.status().as_u16() == 404 {
            skipped += 1;
        } else {
            failed += 1;
        }
    }

    let quota_after = if dry_run {
        quota_before
    } else {
        get_storage_quota(state).ok()
    };

    audit::log_action(
        state,
        "drive_trash_move",
        &serde_json::json!({
            "trashed": trashed,
            "skipped": skipped,
            "failed": failed,
            "bytes_freed": bytes_freed,
        }),
        dry_run,
    )?;

    Ok(TrashResult {
        trashed_count: trashed,
        skipped_count: skipped,
        failed_count: failed,
        bytes_freed,
        dry_run,
        quota_after,
    })
}

pub fn download_file(access_token: &str, file_id: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let url = format!("https://www.googleapis.com/drive/v3/files/{file_id}?alt=media");
    let client = Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .map_err(|e| format!("download failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Google Drive download error {status}: {body}"));
    }

    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    std::fs::write(dest, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

// Backward compat for UI field name
impl DriveAuthStatus {
    #[allow(dead_code)]
    pub fn write_enabled(&self) -> bool {
        self.cleanup_enabled
    }
}

pub const WRITE_SCOPE_URL: &str = DRIVE_FULL_SCOPE;
