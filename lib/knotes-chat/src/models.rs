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
    /// How hard it may reason (`reasoning.effort`: "low", "medium", …), as the list orders them.
    pub efforts: Vec<String>,
    /// The effort it takes when none is asked for.
    pub default_effort: String,
}

/// The efforts every reasoning model takes, for a model the list says nothing of; the middle
/// one is the API's own default.
const EFFORTS: [&str; 3] = ["low", "medium", "high"];

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
            // Each level an object (`{"effort", "description"}`) or the effort alone.
            let listed: Vec<String> = m
                .get("supported_reasoning_levels")
                .and_then(Value::as_array)
                .map(|levels| levels.iter().filter_map(|l| l.get("effort").unwrap_or(l).as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let efforts = if listed.is_empty() { EFFORTS.map(str::to_string).to_vec() } else { listed };
            let default_effort = m
                .get("default_reasoning_level")
                .and_then(Value::as_str)
                .filter(|d| efforts.iter().any(|e| e == d))
                .map(str::to_string)
                .unwrap_or_else(|| efforts.iter().find(|e| *e == "medium").unwrap_or(&efforts[0]).clone());
            Some(Model { slug, name, context_window, efforts, default_effort })
        })
        .collect())
}
