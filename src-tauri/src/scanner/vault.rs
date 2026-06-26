use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::hash;
use crate::oauth::drive;
use crate::state::AppState;

use super::{SourceType, MEDIA_EXTENSIONS};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultCopyResult {
    pub copied_count: i64,
    pub skipped_count: i64,
    pub verified_count: i64,
    pub failed_count: i64,
    pub dry_run: bool,
    pub destination: String,
}

/// Human-readable label for where a file lives.
pub fn source_label(source_type: &str, name: &str) -> String {
    match source_type {
        "google_drive" => format!("Google Drive ({name})"),
        "google_photos" => format!("Google Photos ({name})"),
        "gmail_attachments" => format!("Gmail attachments ({name})"),
        "android_mtp" => format!("Your phone ({name})"),
        "phone_import" => format!("Phone backup folder ({name})"),
        "local" => format!("This PC ({name})"),
        _ => name.to_string(),
    }
}

pub fn vault_subfolder(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::GoogleDrive => "from-google-drive",
        SourceType::AndroidMtp => "from-phone",
        SourceType::PhoneImport => "from-phone-backup",
        SourceType::Local => "from-this-pc",
    }
}

/// Resolve the on-disk path for a scanned file (not Drive — those must be downloaded).
pub fn resolve_local_path(
    source_type: &str,
    config: &serde_json::Value,
    relative_path: &str,
) -> Option<PathBuf> {
    match source_type {
        "local" | "phone_import" => {
            let root = config.get("path")?.as_str()?;
            Some(PathBuf::from(root).join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR)))
        }
        "android_mtp" => {
            let direct = PathBuf::from(relative_path);
            if direct.exists() {
                return Some(direct);
            }
            config
                .get("storage_path")
                .and_then(|v| v.as_str())
                .map(|root| {
                    PathBuf::from(root).join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR))
                })
        }
        _ => None,
    }
}

fn sanitize_vault_relative(relative_path: &str, filename: &str) -> PathBuf {
    let raw = if relative_path.is_empty() {
        filename.to_string()
    } else {
        relative_path.replace('\\', "/")
    };
    let mut parts: Vec<&str> = raw
        .split('/')
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect();
    if parts.is_empty() {
        parts.push(filename);
    }
    parts.iter().fold(PathBuf::new(), |acc, p| acc.join(p))
}

pub fn copy_file_to_vault(
    src: &Path,
    vault_root: &Path,
    source_type: &SourceType,
    relative_path: &str,
    filename: &str,
    dry_run: bool,
) -> Result<VaultCopyOutcome, String> {
    if !src.exists() {
        return Ok(VaultCopyOutcome::Skipped);
    }

    let sub = vault_subfolder(source_type);
    let rel = sanitize_vault_relative(relative_path, filename);
    let dest = vault_root.join(sub).join(rel);

    if dest.exists() {
        if let Ok(fp) = hash::fingerprint_file(&dest) {
            if let Ok(src_fp) = hash::fingerprint_file(src) {
                if fp.md5_hex == src_fp.md5_hex {
                    return Ok(VaultCopyOutcome::Skipped);
                }
            }
        }
    }

    if dry_run {
        return Ok(VaultCopyOutcome::Copied);
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(src, &dest).map_err(|e| format!("copy failed: {e}"))?;

    let src_fp = hash::fingerprint_file(src).map_err(|e| e.to_string())?;
    let dest_fp = hash::fingerprint_file(&dest).map_err(|e| e.to_string())?;
    if src_fp.md5_hex != dest_fp.md5_hex {
        let _ = std::fs::remove_file(&dest);
        return Err(format!(
            "Copy verification failed for {} — file was not saved",
            filename
        ));
    }

    Ok(VaultCopyOutcome::Verified)
}

pub enum VaultCopyOutcome {
    Copied,
    Verified,
    Skipped,
}

pub fn download_drive_file_to_vault(
    state: &Arc<AppState>,
    drive_file_id: &str,
    vault_root: &Path,
    relative_path: &str,
    filename: &str,
    dry_run: bool,
) -> Result<VaultCopyOutcome, String> {
    let sub = vault_subfolder(&SourceType::GoogleDrive);
    let rel = sanitize_vault_relative(relative_path, filename);
    let dest = vault_root.join(sub).join(rel);

    if dest.exists() {
        return Ok(VaultCopyOutcome::Skipped);
    }

    if dry_run {
        return Ok(VaultCopyOutcome::Copied);
    }

    let token = drive::get_valid_access_token(state)?;
    drive::download_file(&token, drive_file_id, &dest)?;

    Ok(VaultCopyOutcome::Verified)
}

pub fn is_media_filename(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    MEDIA_EXTENSIONS.contains(&ext.as_str())
}
