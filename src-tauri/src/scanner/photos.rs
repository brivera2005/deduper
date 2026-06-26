use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::Deserialize;

use super::{is_media_file, ScannedItem, SourceScanner, SourceType};

const PHOTOS_SEARCH_URL: &str = "https://photoslibrary.googleapis.com/v1/mediaItems:search";

#[derive(Debug, Deserialize)]
struct PhotosSearchResponse {
    #[serde(rename = "mediaItems")]
    media_items: Option<Vec<PhotosMediaItem>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PhotosMediaItem {
    id: String,
    filename: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(rename = "mediaMetadata")]
    media_metadata: Option<PhotosMetadata>,
}

#[derive(Debug, Deserialize)]
struct PhotosMetadata {
    #[serde(rename = "creationTime")]
    creation_time: Option<String>,
    width: Option<String>,
    height: Option<String>,
}

pub struct PhotosScanner {
    access_token: String,
    client: Client,
}

impl PhotosScanner {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: Client::new(),
        }
    }

    fn fetch_page(&self, page_token: Option<&str>) -> Result<PhotosSearchResponse, String> {
        let mut body = serde_json::json!({
            "pageSize": 100,
            "filters": {
                "mediaTypeFilter": { "mediaTypes": ["PHOTO", "VIDEO"] }
            }
        });
        if let Some(token) = page_token {
            body["pageToken"] = serde_json::json!(token);
        }

        let resp = self
            .client
            .post(PHOTOS_SEARCH_URL)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .map_err(|e| format!("Google Photos request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            if status.as_u16() == 403 && text.contains("Photos Library API") {
                return Err(
                    "Google Photos API is not enabled. In Google Cloud Console, enable \"Photos Library API\" for your OAuth app."
                        .into(),
                );
            }
            return Err(format!("Google Photos error {status}: {text}"));
        }

        resp.json::<PhotosSearchResponse>()
            .map_err(|e| format!("failed to parse Google Photos response: {e}"))
    }
}

fn photos_content_hash(filename: &str, creation_time: &str, width: &str, height: &str) -> String {
    let name = filename.trim().to_lowercase();
    format!("likely:{name}:{creation_time}:{width}x{height}")
}

impl SourceScanner for PhotosScanner {
    fn source_type(&self) -> SourceType {
        SourceType::GooglePhotos
    }

    fn list_files(&self) -> Result<Vec<ScannedItem>, String> {
        let mut all = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let page = self.fetch_page(page_token.as_deref())?;
            if let Some(items) = page.media_items {
                for item in items {
                    if !is_media_file(&item.filename)
                        && !item
                            .mime_type
                            .as_deref()
                            .map(|m| m.starts_with("image/") || m.starts_with("video/"))
                            .unwrap_or(false)
                    {
                        continue;
                    }

                    let meta = item.media_metadata.as_ref();
                    let creation = meta
                        .and_then(|m| m.creation_time.as_deref())
                        .unwrap_or("");
                    let width = meta.and_then(|m| m.width.as_deref()).unwrap_or("0");
                    let height = meta.and_then(|m| m.height.as_deref()).unwrap_or("0");
                    let modified_at = meta.and_then(|m| m.creation_time.as_deref()).and_then(|s| {
                        DateTime::parse_from_rfc3339(s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    });

                    let hash = photos_content_hash(&item.filename, creation, width, height);

                    all.push(ScannedItem {
                        relative_path: format!("photos://{}", item.id),
                        filename: item.filename,
                        size_bytes: 0,
                        mime_type: item.mime_type,
                        modified_at,
                        content_hash: Some(hash),
                        md5_checksum: None,
                        drive_file_id: Some(item.id),
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
