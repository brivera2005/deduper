use std::sync::OnceLock;

#[derive(serde::Deserialize)]
struct OAuthDefaults {
    google_client_id: String,
    google_client_secret: String,
}

fn defaults() -> &'static OAuthDefaults {
    static CACHE: OnceLock<OAuthDefaults> = OnceLock::new();
    CACHE.get_or_init(|| {
        serde_json::from_str(include_str!("../../resources/oauth.defaults.json")).unwrap_or(
            OAuthDefaults {
                google_client_id: String::new(),
                google_client_secret: String::new(),
            },
        )
    })
}

pub fn client_id() -> Option<String> {
    let id = defaults().google_client_id.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub fn client_secret() -> Option<String> {
    let secret = defaults().google_client_secret.trim();
    if secret.is_empty() {
        None
    } else {
        Some(secret.to_string())
    }
}

pub fn has_credentials() -> bool {
    client_id().is_some() && client_secret().is_some()
}
