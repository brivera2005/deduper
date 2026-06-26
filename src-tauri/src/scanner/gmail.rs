use reqwest::blocking::Client;
use serde::Deserialize;

use super::{ScannedItem, SourceScanner, SourceType};

const GMAIL_MESSAGES_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const MIN_ATTACHMENT_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct MessageList {
    messages: Option<Vec<MessageRef>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageRef {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MessageDetail {
    id: String,
    payload: Option<MessagePayload>,
    #[serde(rename = "internalDate")]
    internal_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessagePayload {
    parts: Option<Vec<MessagePart>>,
    filename: Option<String>,
    body: Option<PartBody>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessagePart {
    filename: Option<String>,
    body: Option<PartBody>,
    parts: Option<Vec<MessagePart>>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PartBody {
    #[serde(rename = "attachmentId")]
    attachment_id: Option<String>,
    size: Option<u64>,
}

pub struct GmailScanner {
    access_token: String,
    client: Client,
}

impl GmailScanner {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: Client::new(),
        }
    }

    fn list_message_ids(&self, page_token: Option<&str>) -> Result<MessageList, String> {
        let mut url = url::Url::parse(GMAIL_MESSAGES_URL).map_err(|e| e.to_string())?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("q", "has:attachment");
            q.append_pair("maxResults", "100");
            if let Some(token) = page_token {
                q.append_pair("pageToken", token);
            }
        }

        let resp = self
            .client
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| format!("Gmail list failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            if status.as_u16() == 403 {
                return Err(
                    "Gmail API is not enabled. In Google Cloud Console, enable \"Gmail API\" for your OAuth app."
                        .into(),
                );
            }
            return Err(format!("Gmail error {status}: {text}"));
        }

        resp.json::<MessageList>()
            .map_err(|e| format!("Gmail parse error: {e}"))
    }

    fn get_message(&self, id: &str) -> Result<MessageDetail, String> {
        let url = format!("{GMAIL_MESSAGES_URL}/{id}?format=full");
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("Gmail message fetch failed for {id}"));
        }

        resp.json::<MessageDetail>().map_err(|e| e.to_string())
    }
}

fn walk_parts(parts: &[MessagePart], out: &mut Vec<(String, u64, String)>, message_id: &str) {
    for part in parts {
        if let Some(nested) = &part.parts {
            walk_parts(nested, out, message_id);
        }
        let filename = part.filename.as_deref().unwrap_or("").trim();
        if filename.is_empty() {
            continue;
        }
        let size = part.body.as_ref().and_then(|b| b.size).unwrap_or(0);
        if size >= MIN_ATTACHMENT_BYTES {
            out.push((filename.to_string(), size, message_id.to_string()));
        }
    }
}

impl SourceScanner for GmailScanner {
    fn source_type(&self) -> SourceType {
        SourceType::GmailAttachments
    }

    fn list_files(&self) -> Result<Vec<ScannedItem>, String> {
        let mut all = Vec::new();
        let mut page_token: Option<String> = None;
        let mut scanned_messages = 0usize;
        const MAX_MESSAGES: usize = 200;

        loop {
            let page = self.list_message_ids(page_token.as_deref())?;
            let Some(refs) = page.messages else {
                break;
            };

            for msg_ref in refs {
                if scanned_messages >= MAX_MESSAGES {
                    break;
                }
                scanned_messages += 1;

                let detail = match self.get_message(&msg_ref.id) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                let mut attachments = Vec::new();
                if let Some(payload) = &detail.payload {
                    if let Some(parts) = &payload.parts {
                        walk_parts(parts, &mut attachments, &detail.id);
                    } else if let Some(name) = &payload.filename {
                        if !name.is_empty() {
                            let size = payload.body.as_ref().and_then(|b| b.size).unwrap_or(0);
                            if size >= MIN_ATTACHMENT_BYTES {
                                attachments.push((name.clone(), size, detail.id.clone()));
                            }
                        }
                    }
                }

                for (filename, size, message_id) in attachments {
                    let hash = format!("gmail:{message_id}:{}", filename.to_lowercase());
                    all.push(ScannedItem {
                        relative_path: format!("gmail://{message_id}/{filename}"),
                        filename,
                        size_bytes: size,
                        mime_type: None,
                        modified_at: None,
                        content_hash: Some(hash),
                        md5_checksum: None,
                        drive_file_id: None,
                        local_path: None,
                    });
                }
            }

            if scanned_messages >= MAX_MESSAGES {
                break;
            }

            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(all)
    }
}
