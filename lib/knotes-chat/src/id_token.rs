use serde_json::Value;

use crate::base64url;

pub const ISSUER: &str = "https://auth.openai.com";

/// Who signed in, from a checked ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdClaims {
    pub subject: String,
    pub email: Option<String>,
}

/// Checks the claims of an ID token that came straight from the token endpoint over TLS
/// (its signature left to TLS, ADR 0027): issuer, audience, expiry, nonce.
pub fn check_id_token(token: &str, client_id: &str, nonce: &str, now: u64) -> Result<IdClaims, String> {
    let payload = token.split('.').nth(1).ok_or("not a JWT")?;
    let claims: Value = base64url::decode(payload).and_then(|b| serde_json::from_slice(&b).ok()).ok_or("unreadable claims")?;
    if claims["iss"] != ISSUER {
        return Err(format!("issuer {}", claims["iss"]));
    }
    let audience = match &claims["aud"] {
        Value::String(a) => a == client_id,
        Value::Array(list) => list.iter().any(|a| a == client_id),
        _ => false,
    };
    if !audience {
        return Err(format!("audience {}", claims["aud"]));
    }
    if claims["exp"].as_u64().is_none_or(|exp| exp <= now) {
        return Err("expired".into());
    }
    if claims["nonce"] != nonce {
        return Err("nonce".into());
    }
    let subject = claims["sub"].as_str().filter(|s| !s.is_empty()).ok_or("no subject")?.to_string();
    Ok(IdClaims { subject, email: claims["email"].as_str().map(str::to_string) })
}
