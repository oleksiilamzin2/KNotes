use serde_json::{Value, json};

/// A signed-in account's tokens (secret: kept in the Keychain).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    /// The issued client the tokens belong to.
    pub client_id: String,
    /// The account (the ID token's `sub`).
    pub subject: String,
    pub email: Option<String>,
    /// Kept to name the account on a later sign-in.
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    /// When the access token runs out (seconds since 1970).
    pub expires_at: u64,
    pub scopes: Vec<String>,
}

impl Credentials {
    pub fn to_json(&self) -> String {
        json!({
            "clientId": self.client_id, "subject": self.subject, "email": self.email, "idToken": self.id_token,
            "accessToken": self.access_token, "refreshToken": self.refresh_token, "expiresAt": self.expires_at, "scopes": self.scopes,
        })
        .to_string()
    }

    pub fn from_json(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self {
            client_id: text("clientId")?,
            subject: text("subject")?,
            email: text("email"),
            id_token: text("idToken").unwrap_or_default(),
            access_token: text("accessToken")?,
            refresh_token: text("refreshToken")?,
            expires_at: value.get("expiresAt").and_then(Value::as_u64).unwrap_or(0),
            scopes: value.get("scopes").and_then(Value::as_array).map(|s| s.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
        })
    }
}
