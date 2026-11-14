use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Value, json};

use crate::crypto::uuid_urn;

pub const REGISTRATION_FILE: &str = "chatgpt.json";

/// What is not secret about signing in, kept in Knotes's data: this host's id, the client id
/// the first sign-in got (`oaiapp_…`), the account it is for, and whether the first-use
/// notice was shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registration {
    pub host_id: String,
    pub client_id: Option<String>,
    pub email: Option<String>,
    pub subject: Option<String>,
    pub welcomed: bool,
}

impl Registration {
    /// The saved registration; a missing or broken file gives a new host id (saved with the
    /// first sign-in).
    pub fn load(app_data: &Path) -> Self {
        let value: Value = fs::read_to_string(app_data.join(REGISTRATION_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let text = |key: &str| value.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        Self {
            host_id: text("hostId").unwrap_or_else(uuid_urn),
            client_id: text("clientId"),
            email: text("email"),
            subject: text("subject"),
            welcomed: value.get("welcomed").and_then(Value::as_bool).unwrap_or(false),
        }
    }

    /// Written to a temporary file and renamed.
    pub fn save(&self, app_data: &Path) -> io::Result<()> {
        fs::create_dir_all(app_data)?;
        let value = json!({"hostId": self.host_id, "clientId": self.client_id, "email": self.email, "subject": self.subject, "welcomed": self.welcomed});
        let temporary = app_data.join(format!("{REGISTRATION_FILE}.tmp"));
        fs::write(&temporary, serde_json::to_string_pretty(&value)?)?;
        fs::rename(temporary, app_data.join(REGISTRATION_FILE))
    }
}
