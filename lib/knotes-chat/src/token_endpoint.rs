//! OpenAI's OAuth endpoints and the token endpoint's replies.

use std::io::Read;

use serde_json::Value;

use crate::form::form_encode;
use crate::{ChatError, HttpRequest, HttpResponse, Method, Transport};

pub(crate) const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
pub(crate) const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub(crate) const DISCOVERY: &str = "https://auth.openai.com/.well-known/openid-configuration";
/// What the tokens are for.
pub(crate) const RESOURCE: &str = "https://api.openai.com/v1";
pub(crate) const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// The scope that lets requests use the ChatGPT plan.
pub(crate) const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";

pub(crate) struct TokenReply {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: u64,
    pub scopes: Option<Vec<String>>,
}

pub(crate) enum TokenFailure {
    /// The endpoint said no (its error code and words).
    Refused(String, String),
    /// A server error: try again later.
    Unavailable(String),
    Network(String),
}

pub(crate) fn post_form(transport: &dyn Transport, url: &str, fields: &[(&str, &str)]) -> Result<HttpResponse, ChatError> {
    let request = HttpRequest {
        method: Method::Post,
        url: url.to_string(),
        headers: vec![("Content-Type".into(), "application/x-www-form-urlencoded".into()), ("Accept".into(), "application/json".into())],
        body: form_encode(fields).into_bytes(),
    };
    transport.send(&request)
}

pub(crate) fn read_json(response: &mut HttpResponse) -> Value {
    let mut body = Vec::new();
    let _ = (&mut response.body).take(1 << 20).read_to_end(&mut body);
    serde_json::from_slice(&body).unwrap_or_default()
}

/// Posts `fields` to the token endpoint.
pub(crate) fn post_token(transport: &dyn Transport, fields: &[(&str, &str)]) -> Result<TokenReply, TokenFailure> {
    let mut response = post_form(transport, TOKEN, fields).map_err(|e| match e {
        ChatError::Network(words) => TokenFailure::Network(words),
        other => TokenFailure::Network(other.to_string()),
    })?;
    let value = read_json(&mut response);
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    if !(200..300).contains(&response.status) {
        let (code, words) = match value.get("error") {
            Some(Value::String(code)) => (code.clone(), text(&value, "error_description").unwrap_or_default()),
            Some(error) => (text(error, "code").unwrap_or_default(), text(error, "message").unwrap_or_default()),
            None => (String::new(), format!("HTTP {}", response.status)),
        };
        return Err(if response.status >= 500 { TokenFailure::Unavailable(words) } else { TokenFailure::Refused(code, words) });
    }
    let access_token = text(&value, "access_token").ok_or_else(|| TokenFailure::Refused(String::new(), "no access token".into()))?;
    Ok(TokenReply {
        access_token,
        refresh_token: text(&value, "refresh_token"),
        id_token: text(&value, "id_token"),
        expires_in: value.get("expires_in").and_then(Value::as_u64).unwrap_or(3600),
        scopes: text(&value, "scope").map(|s| s.split_whitespace().map(str::to_string).collect()),
    })
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
