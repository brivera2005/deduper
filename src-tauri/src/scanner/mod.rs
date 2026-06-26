pub mod drive;
pub mod engine;
pub mod full_audit;
pub mod gmail;
pub mod local;
pub mod mtp;
pub mod photos;
pub mod vault;

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Local,
    GoogleDrive,
    GooglePhotos,
    GmailAttachments,
    AndroidMtp,
    PhoneImport,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::GoogleDrive => "google_drive",
            Self::GooglePhotos => "google_photos",
            Self::GmailAttachments => "gmail_attachments",
            Self::AndroidMtp => "android_mtp",
            Self::PhoneImport => "phone_import",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "local" => Some(Self::Local),
            "google_drive" => Some(Self::GoogleDrive),
            "google_photos" => Some(Self::GooglePhotos),
            "gmail_attachments" => Some(Self::GmailAttachments),
            "android_mtp" => Some(Self::AndroidMtp),
            "phone_import" => Some(Self::PhoneImport),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    VerifiedDuplicate,
    LikelyDuplicate,
    Unique,
    Unknown,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VerifiedDuplicate => "verified_duplicate",
            Self::LikelyDuplicate => "likely_duplicate",
            Self::Unique => "unique",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "verified_duplicate" => Self::VerifiedDuplicate,
            "likely_duplicate" => Self::LikelyDuplicate,
            "unique" => Self::Unique,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    pub id: String,
    pub source_type: SourceType,
    pub name: String,
    pub config: serde_json::Value,
    pub status: String,
    pub last_scan_at: Option<String>,
    pub file_count: i64,
    pub total_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub source_id: String,
    pub relative_path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub mime_type: Option<String>,
    pub modified_at: Option<String>,
    pub content_hash: Option<String>,
    pub md5_checksum: Option<String>,
    pub drive_file_id: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub job_id: String,
    pub source_id: String,
    pub status: String,
    pub total_files: i64,
    pub processed_files: i64,
    pub hashed_files: i64,
    pub current_file: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedItem {
    pub relative_path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub modified_at: Option<DateTime<Utc>>,
    pub content_hash: Option<String>,
    pub md5_checksum: Option<String>,
    pub drive_file_id: Option<String>,
    pub local_path: Option<PathBuf>,
}

pub trait SourceScanner: Send + Sync {
    fn source_type(&self) -> SourceType;
    fn list_files(&self) -> Result<Vec<ScannedItem>, String>;
    fn read_file_for_hash(&self, item: &ScannedItem) -> Result<PathBuf, String> {
        item.local_path
            .clone()
            .ok_or_else(|| "no local path for hashing".to_string())
    }
}

pub const MEDIA_EXTENSIONS: &[&str] = &[
    // photos
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tif", "tiff", "raw", "cr2",
    "nef", "arw", "dng",
    // videos
    "mp4", "mov", "avi", "mkv", "wmv", "flv", "webm", "m4v", "3gp", "mts", "m2ts",
    // documents
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "rtf", "odt", "ods", "odp",
    "csv", "md",
];

pub fn is_media_file(name: &str) -> bool {
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    MEDIA_EXTENSIONS.contains(&ext.as_str())
}
