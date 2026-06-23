use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().expect("project root");

    let (client_id, client_secret, source) = load_oauth_credentials(project_root);

    let resources_dir = manifest_dir.join("resources");
    fs::create_dir_all(&resources_dir).expect("create resources dir");

    let oauth_json = serde_json::json!({
        "google_client_id": client_id,
        "google_client_secret": client_secret,
    });
    fs::write(
        resources_dir.join("oauth.defaults.json"),
        serde_json::to_string_pretty(&oauth_json).expect("serialize oauth defaults"),
    )
    .expect("write oauth.defaults.json");

    if client_id.is_empty() || client_secret.is_empty() {
        println!(
            "cargo:warning=Deduper OAuth: no credentials at build time. Add GOOGLE_CLIENT_ID/SECRET to .env or deduper-oauth.json before dad-ready release builds."
        );
    } else {
        println!("cargo:warning=Deduper OAuth: embedded credentials from {source}");
    }

    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-changed=../deduper-oauth.json");
    if let Ok(appdata) = env::var("APPDATA") {
        let config = PathBuf::from(appdata)
            .join("com.deduper.app")
            .join("config.json");
        if config.exists() {
            println!("cargo:rerun-if-changed={}", config.display());
        }
    }

    tauri_build::build();
}

fn load_oauth_credentials(project_root: &Path) -> (String, String, &'static str) {
    if let Some((id, secret)) = load_from_json_file(&project_root.join("deduper-oauth.json")) {
        return (id, secret, "deduper-oauth.json");
    }
    if let Some((id, secret)) = load_from_dotenv(&project_root.join(".env")) {
        return (id, secret, ".env");
    }
    if let Ok(appdata) = env::var("APPDATA") {
        let config_path = PathBuf::from(appdata)
            .join("com.deduper.app")
            .join("config.json");
        if let Some((id, secret)) = load_from_json_file(&config_path) {
            return (id, secret, "AppData config.json");
        }
    }
    (String::new(), String::new(), "none")
}

fn load_from_json_file(path: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let id = value
        .get("google_client_id")
        .or_else(|| value.get("GOOGLE_CLIENT_ID"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && !is_placeholder(s))?
        .to_string();
    let secret = value
        .get("google_client_secret")
        .or_else(|| value.get("GOOGLE_CLIENT_SECRET"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && !is_placeholder(s))?
        .to_string();
    Some((id, secret))
}

fn load_from_dotenv(path: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(path).ok()?;
    let mut id = None;
    let mut secret = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key.trim() {
                "GOOGLE_CLIENT_ID" if !value.is_empty() && !is_placeholder(value) => {
                    id = Some(value.to_string());
                }
                "GOOGLE_CLIENT_SECRET" if !value.is_empty() && !is_placeholder(value) => {
                    secret = Some(value.to_string());
                }
                _ => {}
            }
        }
    }
    match (id, secret) {
        (Some(id), Some(secret)) => Some((id, secret)),
        _ => None,
    }
}

fn is_placeholder(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("your-")
        || lower.contains("your_")
        || lower.contains("example")
        || lower.contains("placeholder")
        || lower.contains("changeme")
}
