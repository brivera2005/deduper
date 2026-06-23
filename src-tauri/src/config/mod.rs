use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub google_client_id: Option<String>,
    #[serde(default)]
    pub google_client_secret: Option<String>,
    #[serde(default)]
    pub vault_path: Option<String>,
    #[serde(default)]
    pub wizard_completed_at: Option<String>,
    #[serde(default)]
    pub wizard_skipped: bool,
}

impl AppConfig {
    pub fn config_path(data_dir: &Path) -> PathBuf {
        data_dir.join("config.json")
    }

    pub fn load(data_dir: &Path) -> Self {
        let path = Self::config_path(data_dir);
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str(&text) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let path = Self::config_path(data_dir);
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn has_google_credentials(&self) -> bool {
        self.google_client_id
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
            && self
                .google_client_secret
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
    }

    pub fn google_client_id(&self) -> Option<String> {
        if let Some(id) = &self.google_client_id {
            if !id.trim().is_empty() {
                return Some(id.trim().to_string());
            }
        }
        std::env::var("GOOGLE_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn google_client_secret(&self) -> Option<String> {
        if let Some(secret) = &self.google_client_secret {
            if !secret.trim().is_empty() {
                return Some(secret.trim().to_string());
            }
        }
        std::env::var("GOOGLE_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn wizard_done(&self) -> bool {
        self.wizard_completed_at.is_some() || self.wizard_skipped
    }
}

#[derive(Debug, Serialize)]
pub struct GoogleOAuthConfigStatus {
    pub configured: bool,
    pub client_id_preview: Option<String>,
    pub source: String,
}

pub fn get_google_oauth_status(data_dir: &Path) -> GoogleOAuthConfigStatus {
    let cfg = AppConfig::load(data_dir);
    let env_id = std::env::var("GOOGLE_CLIENT_ID").ok().filter(|s| !s.is_empty());
    let env_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty());

    if cfg.has_google_credentials() {
        let preview = cfg.google_client_id.as_ref().map(|id| {
            if id.len() > 12 {
                format!("{}…", &id[..12])
            } else {
                id.clone()
            }
        });
        return GoogleOAuthConfigStatus {
            configured: true,
            client_id_preview: preview,
            source: "config".into(),
        };
    }

    if env_id.is_some() && env_secret.is_some() {
        let preview = env_id.map(|id| {
            if id.len() > 12 {
                format!("{}…", &id[..12])
            } else {
                id
            }
        });
        return GoogleOAuthConfigStatus {
            configured: true,
            client_id_preview: preview,
            source: "env".into(),
        };
    }

    GoogleOAuthConfigStatus {
        configured: false,
        client_id_preview: None,
        source: "none".into(),
    }
}

pub fn save_google_credentials(
    data_dir: &Path,
    client_id: String,
    client_secret: String,
) -> Result<(), String> {
    let mut cfg = AppConfig::load(data_dir);
    cfg.google_client_id = Some(client_id.trim().to_string());
    cfg.google_client_secret = Some(client_secret.trim().to_string());
    cfg.save(data_dir)
}

pub fn set_vault_path(data_dir: &Path, path: String) -> Result<(), String> {
    let mut cfg = AppConfig::load(data_dir);
    cfg.vault_path = Some(path);
    cfg.save(data_dir)
}

pub fn complete_wizard(data_dir: &Path, skipped: bool) -> Result<(), String> {
    let mut cfg = AppConfig::load(data_dir);
    if skipped {
        cfg.wizard_skipped = true;
    } else {
        cfg.wizard_completed_at = Some(crate::db::now_iso());
        cfg.wizard_skipped = false;
    }
    cfg.save(data_dir)
}

pub fn reset_wizard(data_dir: &Path) -> Result<(), String> {
    let mut cfg = AppConfig::load(data_dir);
    cfg.wizard_completed_at = None;
    cfg.wizard_skipped = false;
    cfg.save(data_dir)
}
