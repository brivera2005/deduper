use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HashError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type HashResult<T> = Result<T, HashError>;

const CHUNK_SIZE: usize = 1024 * 1024; // 1 MB

/// Compute SHA-256 hex digest of file contents.
pub fn hash_file(path: &Path) -> HashResult<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(CHUNK_SIZE, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; CHUNK_SIZE];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute SHA-256 from in-memory bytes.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Normalize Drive md5Checksum to lowercase hex (Drive returns base64 for some files).
pub fn normalize_md5(raw: &str) -> String {
    raw.trim().to_lowercase()
}
