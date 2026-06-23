use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use super::{is_media_file, ScannedItem, SourceScanner, SourceType};

pub struct LocalScanner {
    root: PathBuf,
}

impl LocalScanner {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl SourceScanner for LocalScanner {
    fn source_type(&self) -> SourceType {
        SourceType::Local
    }

    fn list_files(&self) -> Result<Vec<ScannedItem>, String> {
        if !self.root.exists() {
            return Err(format!("folder not found: {}", self.root.display()));
        }

        let mut items = Vec::new();
        for entry in WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if !is_media_file(&filename) {
                continue;
            }

            let metadata = entry
                .metadata()
                .map_err(|e| format!("metadata error: {e}"))?;

            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let modified_at = metadata.modified().ok().map(|t| {
                let duration = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                DateTime::<Utc>::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
                    .unwrap_or_else(Utc::now)
            });

            let mime_type = mime_guess::from_path(path)
                .first()
                .map(|m| m.to_string());

            items.push(ScannedItem {
                relative_path: relative,
                filename,
                size_bytes: metadata.len(),
                mime_type,
                modified_at,
                content_hash: None,
                md5_checksum: None,
                drive_file_id: None,
                local_path: Some(path.to_path_buf()),
            });
        }

        Ok(items)
    }

    fn read_file_for_hash(&self, item: &ScannedItem) -> Result<PathBuf, String> {
        if let Some(p) = &item.local_path {
            return Ok(p.clone());
        }
        Ok(self.root.join(&item.relative_path))
    }
}

/// Phone import uses the same walk logic as local folders.
pub struct PhoneImportScanner {
    inner: LocalScanner,
}

impl PhoneImportScanner {
    pub fn new(root: PathBuf) -> Self {
        Self {
            inner: LocalScanner::new(root),
        }
    }
}

impl SourceScanner for PhoneImportScanner {
    fn source_type(&self) -> SourceType {
        SourceType::PhoneImport
    }

    fn list_files(&self) -> Result<Vec<ScannedItem>, String> {
        self.inner.list_files()
    }

    fn read_file_for_hash(&self, item: &ScannedItem) -> Result<PathBuf, String> {
        self.inner.read_file_for_hash(item)
    }
}

pub fn validate_folder(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("folder does not exist".into());
    }
    if !path.is_dir() {
        return Err("path is not a folder".into());
    }
    Ok(())
}
