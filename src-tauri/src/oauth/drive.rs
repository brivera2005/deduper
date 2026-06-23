use std::sync::Arc;

use reqwest::blocking::Client;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, Server};

use crate::audit;
use crate::db::now_iso;
use crate::state::AppState;

const PROVIDER: &str = "google_drive";
const READONLY_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const WRITE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

#[derive(Debug, Serialize, Deserialize)]
pub struct DriveAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub scopes: Vec<String>,
    pub write_enabled: bool,
}

fn oauth_port() -> u16 {
    std::env::var("DEDUPER_OAUTH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8888)
}

fn load_client_credentials(state: &AppState) -> Result<(String, String), String> {
    let cfg = crate::config::AppConfig::load(&state.data_dir);
    let id = cfg
        .google_client_id()
        .ok_or("Google Drive sign-in is not available in this build. Reinstall from a release build, or add your own OAuth app under Advanced in setup.")?;
    let secret = cfg
        .google_client_secret()
        .ok_or("Google Drive sign-in is not available in this build. Reinstall from a release build, or add your own OAuth app under Advanced in setup.")?;
    Ok((id, secret))
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
            write_enabled: false,
        }),
        Some((token, scopes)) => {
            let scope_list: Vec<String> = scopes.split_whitespace().map(String::from).collect();
            let email = fetch_token_email(&token).ok();
            Ok(DriveAuthStatus {
                connected: true,
                email,
                write_enabled: scope_list.iter().any(|s| s.contains("drive.file")),
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
        q.append_pair("scope", READONLY_SCOPE);
        q.append_pair("code_challenge", pkce_challenge.as_str());
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
    }

    open::that(auth_url.as_str()).map_err(|e| format!("failed to open browser: {e}"))?;

    let code = wait_for_callback(port)?;

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

    let email = fetch_token_email(&tokens.access_token).ok();

    store_tokens(
        &state,
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
        expires_at.as_deref(),
        tokens.scope.as_deref().unwrap_or(READONLY_SCOPE),
    )?;

    ensure_drive_source(&state)?;

    audit::log_action(
        &state,
        "drive_connected",
        &serde_json::json!({ "email": email, "scope": READONLY_SCOPE }),
        true,
    )?;

    Ok(DriveAuthStatus {
        connected: true,
        email,
        scopes: vec![READONLY_SCOPE.into()],
        write_enabled: false,
    })
}

fn wait_for_callback(port: u16) -> Result<String, String> {
    let server = Server::http(format!("127.0.0.1:{port}"))
        .map_err(|e| format!("failed to start OAuth callback server on port {port}: {e}"))?;

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        if let Some(query) = url.split('?').nth(1) {
            for pair in query.split('&') {
                if let Some(code) = pair.strip_prefix("code=") {
                    let code = decode_url_component(code);
                    let html = "<html><body style='font-family:sans-serif;text-align:center;padding:3rem'>
                        <h2>Deduper connected!</h2>
                        <p>You can close this tab and return to Deduper.</p></body></html>";
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

fn ensure_drive_source(state: &AppState) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sources WHERE source_type = 'google_drive'",
            [],
            |row| row.get(0),
        )?;

        if exists == 0 {
            conn.execute(
                "INSERT INTO sources (id, source_type, name, config_json, status, created_at)
                 VALUES (?1, 'google_drive', 'Google Drive', '{}', 'idle', ?2)",
                params![uuid::Uuid::new_v4().to_string(), now_iso()],
            )?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn disconnect_drive(state: &AppState) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.with_conn(|conn| {
        conn.execute("DELETE FROM oauth_tokens WHERE provider = ?1", params![PROVIDER])?;
        Ok(())
    })
    .map_err(|e| e.to_string())?;

    audit::log_action(
        state,
        "drive_disconnected",
        &serde_json::json!({}),
        true,
    )
}

pub fn get_valid_access_token(state: &AppState) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let (access_token, refresh_token, expires_at) = db
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT access_token, refresh_token, expires_at FROM oauth_tokens WHERE provider = ?1",
            )?;
            stmt.query_row(params![PROVIDER], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(Into::into)
        })
        .map_err(|e| e.to_string())?;

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

    let refresh = refresh_token.ok_or("no refresh token — reconnect Google Drive")?;
    refresh_access_token(state, &refresh)
}

fn refresh_access_token(state: &AppState, refresh_token: &str) -> Result<String, String> {
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
        return Err("token refresh failed — reconnect Google Drive".into());
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
        READONLY_SCOPE,
    )?;

    Ok(body.access_token)
}

#[allow(dead_code)]
pub fn request_write_scope_note() -> &'static str {
    "Move to Drive Trash requires separate OAuth consent with drive.file scope. Never auto-deletes."
}

pub const WRITE_SCOPE_URL: &str = WRITE_SCOPE;
