use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::Deserialize;

use super::{is_media_file, ScannedItem, SourceScanner, SourceType};

const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const PAGE_SIZE: u32 = 200;

#[derive(Debug, Deserialize)]
struct DriveFileList {
    files: Option<Vec<DriveFile>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    size: Option<String>,
    #[serde(rename = "md5Checksum")]
    md5_checksum: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

pub struct DriveScanner {
    access_token: String,
    client: Client,
}

impl DriveScanner {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: Client::new(),
        }
    }

    fn fetch_page(&self, page_token: Option<&str>) -> Result<DriveFileList, String> {
        let mut query = url::Url::parse(DRIVE_FILES_URL).map_err(|e| e.to_string())?;
        {
            let mut pairs = query.query_pairs_mut();
            pairs.append_pair("pageSize", &PAGE_SIZE.to_string());
            pairs.append_pair(
                "fields",
                "nextPageToken,files(id,name,mimeType,size,md5Checksum,modifiedTime)",
            );
            pairs.append_pair(
                "q",
                "trashed = false and mimeType != 'application/vnd.google-apps.folder'",
            );
            if let Some(token) = page_token {
                pairs.append_pair("pageToken", token);
            }
        }

        let resp = self
            .client
            .get(query)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Drive API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Drive API error {status}: {body}"));
        }

        resp.json::<DriveFileList>()
            .map_err(|e| format!("failed to parse Drive response: {e}"))
    }
}

impl SourceScanner for DriveScanner {
    fn source_type(&self) -> SourceType {
        SourceType::GoogleDrive
    }

    fn list_files(&self) -> Result<Vec<ScannedItem>, String> {
        let mut all = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let page = self.fetch_page(page_token.as_deref())?;
            if let Some(files) = page.files {
                for f in files {
                    if f.mime_type.as_deref() == Some("application/vnd.google-apps.folder") {
                        continue;
                    }
                    if !is_media_file(&f.name)
                        && !f
                            .mime_type
                            .as_deref()
                            .map(|m| m.starts_with("image/") || m.starts_with("video/"))
                            .unwrap_or(false)
                    {
                        continue;
                    }

                    let size_bytes = f.size.as_deref().unwrap_or("0").parse().unwrap_or(0);
                    let modified_at = f.modified_time.as_deref().and_then(|s| {
                        DateTime::parse_from_rfc3339(s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    });

                    all.push(ScannedItem {
                        relative_path: format!("drive://{}", f.id),
                        filename: f.name,
                        size_bytes,
                        mime_type: f.mime_type,
                        modified_at,
                        content_hash: None,
                        md5_checksum: f.md5_checksum,
                        drive_file_id: Some(f.id),
                        local_path: None,
                    });
                }
            }

            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(all)
    }
}
