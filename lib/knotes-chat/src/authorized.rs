use crate::{AccessToken, ChatError, HttpRequest, HttpResponse, Transport};

/// Sends `request` with the user's token; a 401 renews the token and sends it once more.
pub(crate) fn send_authorized(transport: &dyn Transport, token: &dyn AccessToken, mut request: HttpRequest) -> Result<HttpResponse, ChatError> {
    request.headers.push(("Authorization".into(), format!("Bearer {}", token.token()?)));
    let response = transport.send(&request)?;
    if response.status != 401 {
        return Ok(response);
    }
    let renewed = token.renew()?;
    request.headers.retain(|(k, _)| k != "Authorization");
    request.headers.push(("Authorization".into(), format!("Bearer {renewed}")));
    transport.send(&request)
}

/// The error a response other than 2xx carries.
pub(crate) fn error_of(mut response: HttpResponse) -> ChatError {
    let mut body = Vec::new();
    let _ = std::io::Read::read_to_end(&mut std::io::Read::take(&mut response.body, 64 * 1024), &mut body);
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let error = match value.get("error") {
        Some(e) if e.is_object() => e.clone(),
        _ => serde_json::json!({"message": value.get("detail").and_then(|d| d.as_str()).unwrap_or_default()}),
    };
    ChatError::from_openai(&error, response.status)
}
