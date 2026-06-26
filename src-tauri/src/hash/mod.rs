use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use digest::Digest;
use md5::Md5;
use sha2::Sha256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HashError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type HashResult<T> = Result<T, HashError>;

const CHUNK_SIZE: usize = 1024 * 1024; // 1 MB

/// Result of fingerprinting a file for cross-source duplicate matching.
#[derive(Debug, Clone)]
pub struct FileFingerprint {
    /// Canonical identity used for duplicate grouping (`md5:hex`).
    pub content_hash: String,
    /// Raw MD5 hex (lowercase).
    pub md5_hex: String,
    /// SHA-256 hex for optional verification receipts.
    pub sha256_hex: String,
}

/// Read file once and compute MD5 + SHA-256.
pub fn fingerprint_file(path: &Path) -> HashResult<FileFingerprint> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(CHUNK_SIZE, file);
    let mut md5 = Md5::new();
    let mut sha256 = Sha256::new();
    let mut buffer = [0u8; CHUNK_SIZE];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        md5.update(&buffer[..n]);
        sha256.update(&buffer[..n]);
    }

    let md5_hex = format!("{:x}", md5.finalize());
    let sha256_hex = format!("{:x}", sha256.finalize());

    Ok(FileFingerprint {
        content_hash: content_identity_from_md5(&md5_hex),
        md5_hex,
        sha256_hex,
    })
}

/// Legacy helper — prefer `fingerprint_file`.
pub fn hash_file(path: &Path) -> HashResult<String> {
    Ok(fingerprint_file(path)?.content_hash)
}

pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Normalize Drive md5Checksum to lowercase hex.
pub fn normalize_md5(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Canonical content identity shared by Google Drive metadata and local files.
pub fn content_identity_from_md5(md5_hex: &str) -> String {
    format!("md5:{}", normalize_md5(md5_hex))
}

pub fn content_identity_from_drive_checksum(raw: &str) -> String {
    content_identity_from_md5(raw)
}
