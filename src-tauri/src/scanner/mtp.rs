use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

const POWERSHELL_TIMEOUT: Duration = Duration::from_secs(20);

use super::{is_media_file, ScannedItem, SourceScanner, SourceType};

const MEDIA_FOLDERS: &[&str] = &[
    "DCIM",
    "Pictures",
    "Download",
    "Downloads",
    "Movies",
    "Camera",
    "WhatsApp",
    "Screenshots",
    "Snapchat",
    "Instagram",
];

#[derive(Debug, Clone, Deserialize)]
struct MtpDeviceJson {
    name: String,
    storage_name: String,
    storage_path: String,
    free_bytes: Option<u64>,
    total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct MtpFileJson {
    name: String,
    path: String,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct MtpDevice {
    pub name: String,
    pub storage_name: String,
    pub storage_path: String,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

impl From<MtpDeviceJson> for MtpDevice {
    fn from(v: MtpDeviceJson) -> Self {
        Self {
            name: v.name,
            storage_name: v.storage_name,
            storage_path: v.storage_path,
            free_bytes: v.free_bytes,
            total_bytes: v.total_bytes,
        }
    }
}

pub struct MtpScanner {
    _device_name: String,
    storage_path: String,
}

impl MtpScanner {
    pub fn new(device_name: String, storage_path: String) -> Self {
        Self {
            _device_name: device_name,
            storage_path,
        }
    }

    pub fn detect_devices() -> Vec<MtpDevice> {
        let script = include_str!("mtp_detect.ps1");
        let output = match run_powershell_with_timeout(script) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("MTP detect failed: {e}");
                return vec![];
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                eprintln!("MTP detect stderr: {stderr}");
            }
            return vec![];
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() || trimmed == "[]" {
            return vec![];
        }

        serde_json::from_str::<Vec<MtpDeviceJson>>(trimmed)
            .map(|items| items.into_iter().map(MtpDevice::from).collect())
            .unwrap_or_else(|e| {
                eprintln!("MTP detect JSON parse error: {e} — raw: {trimmed}");
                vec![]
            })
    }

    fn list_files_for_storage(storage_path: &str) -> Result<Vec<MtpFileJson>, String> {
        let script = include_str!("mtp_list_files.ps1");
        let command = format!("$storagePath = '{}'; {script}", escape_ps_single(storage_path));
        let output = run_powershell_with_timeout(&command)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "MTP file listing failed: {}",
                if stderr.trim().is_empty() {
                    "no MTP device in file transfer mode".into()
                } else {
                    stderr.trim().to_string()
                }
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() || trimmed == "[]" {
            return Ok(vec![]);
        }

        serde_json::from_str(trimmed)
            .map_err(|e| format!("failed to parse MTP file list: {e}"))
    }
}

fn escape_ps_single(s: &str) -> String {
    s.replace('\'', "''")
}

/// Run PowerShell off the UI thread path with a hard timeout so MTP/COM enumeration cannot hang forever.
fn run_powershell_with_timeout(script: &str) -> Result<std::process::Output, String> {
    let mut child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start PowerShell: {e}"))?;

    let stdout_thread = child.stdout.take().map(|mut out| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            out.read_to_end(&mut buf).ok();
            buf
        })
    });
    let stderr_thread = child.stderr.take().map(|mut err| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            err.read_to_end(&mut buf).ok();
            buf
        })
    });

    let started = Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|e| format!("waiting for PowerShell: {e}"))?
        {
            Some(status) => {
                let stdout = stdout_thread
                    .map(|t| t.join().unwrap_or_default())
                    .unwrap_or_default();
                let stderr = stderr_thread
                    .map(|t| t.join().unwrap_or_default())
                    .unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None if started.elapsed() >= POWERSHELL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Phone detection timed out. Unplug and replug your phone, choose File transfer on the phone, then try again.".into(),
                );
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
}

impl SourceScanner for MtpScanner {
    fn source_type(&self) -> SourceType {
        SourceType::AndroidMtp
    }

    fn list_files(&self) -> Result<Vec<ScannedItem>, String> {
        let raw = Self::list_files_for_storage(&self.storage_path)?;
        if raw.is_empty() {
            return Err(
                "No photos or videos found on the phone. Check USB is set to File Transfer / MTP, \
                 then try again. You can also use Manual Import after copying files to your PC."
                    .into(),
            );
        }

        let mut items = Vec::new();
        for f in raw {
            if !is_media_file(&f.name) {
                continue;
            }
            let path = PathBuf::from(&f.path);
            items.push(ScannedItem {
                relative_path: f.path.clone(),
                filename: f.name,
                size_bytes: f.size_bytes,
                mime_type: mime_guess::from_path(&path).first().map(|m| m.to_string()),
                modified_at: None,
                content_hash: None,
                md5_checksum: None,
                drive_file_id: None,
                local_path: Some(path),
            });
        }

        if items.is_empty() {
            return Err(
                "Phone connected but no supported media files found in DCIM, Pictures, or Download."
                    .into(),
            );
        }

        Ok(items)
    }

    fn read_file_for_hash(&self, item: &ScannedItem) -> Result<PathBuf, String> {
        let path = item
            .local_path
            .clone()
            .ok_or_else(|| format!("no path for {}", item.filename))?;

        if !path.exists() {
            return Err(format!(
                "Phone file no longer available (disconnected?): {}",
                item.filename
            ));
        }

        Ok(path)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MtpDeviceInfo {
    pub name: String,
    pub storage_name: String,
    pub storage_path: String,
    pub connected: bool,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub media_folders: Vec<String>,
}

pub fn get_mtp_status() -> Vec<MtpDeviceInfo> {
    MtpScanner::detect_devices()
        .into_iter()
        .map(|d| MtpDeviceInfo {
            name: d.name.clone(),
            storage_name: d.storage_name.clone(),
            storage_path: d.storage_path.clone(),
            connected: true,
            free_bytes: d.free_bytes,
            total_bytes: d.total_bytes,
            media_folders: MEDIA_FOLDERS.iter().map(|s| s.to_string()).collect(),
        })
        .collect()
}

pub fn format_storage(bytes: Option<u64>) -> String {
    match bytes {
        Some(b) if b >= 1024u64.pow(3) => format!("{:.1} GB", b as f64 / 1024f64.powi(3)),
        Some(b) if b >= 1024u64.pow(2) => format!("{:.0} MB", b as f64 / 1024f64.powi(2)),
        Some(b) => format!("{b} bytes"),
        None => "Unknown".into(),
    }
}

pub fn validate_device_connected(storage_path: &str) -> Result<(), String> {
    let devices = MtpScanner::detect_devices();
    if devices.is_empty() {
        return Err(
            "No Android phone detected. Plug in your phone via USB, unlock it, and choose \
             \"File transfer\" or \"Transfer files\" (not \"Charge only\")."
                .into(),
        );
    }
    if !devices.iter().any(|d| d.storage_path == storage_path) {
        return Err(
            "Phone was disconnected or switched out of file transfer mode. Reconnect and try again."
                .into(),
        );
    }
    Ok(())
}
