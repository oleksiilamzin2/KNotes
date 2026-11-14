use serde_json::Value;

use crate::authorized::{error_of, send_authorized};
use crate::{API, AccessToken, ChatError, HttpRequest, Method, Transport};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// What requests name it by.
    pub slug: String,
    /// What the user sees.
    pub name: String,
    /// How many tokens a request may take, when the list says.
    pub context_window: Option<u64>,
}

/// The models the user's plan offers (those listed, not hidden).
pub fn list_models(transport: &dyn Transport, token: &dyn AccessToken) -> Result<Vec<Model>, ChatError> {
    let request = HttpRequest { method: Method::Get, url: format!("{API}/models"), headers: vec![("Accept".into(), "application/json".into())], body: Vec::new() };
    let mut response = send_authorized(transport, token, request)?;
    if !(200..300).contains(&response.status) {
        return Err(error_of(response));
    }
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut response.body, &mut body).map_err(|e| ChatError::Network(e.to_string()))?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| ChatError::Failed(format!("models: {e}")))?;
    let models = value.get("models").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(models
        .iter()
        .filter(|m| m.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|m| {
            let slug = m.get("slug")?.as_str()?.to_string();
            let name = m.get("display_name").and_then(Value::as_str).unwrap_or(&slug).to_string();
            let context_window = m.get("context_window").or_else(|| m.get("max_context_window")).and_then(Value::as_u64);
            Some(Model { slug, name, context_window })
        })
        .collect())
}
