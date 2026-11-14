//! The helper's protocol: a JSON line in (`{"passages": [{"query"} | {"title", "text"}]}`), a
//! JSON line out (`{"vectors": [[256 bytes]]}` or `{"error"}`).

use serde_json::{Value, json};

use crate::{DIMS, EmbedError, Passage, Vector};

pub fn request(passages: &[Passage]) -> String {
    let passages: Vec<Value> = passages
        .iter()
        .map(|p| match p {
            Passage::Query(text) => json!({"query": text}),
            Passage::Document { title, text } => json!({"title": title, "text": text}),
        })
        .collect();
    json!({"passages": passages}).to_string()
}

pub fn read_request(line: &str) -> Result<Vec<Passage>, EmbedError> {
    let value: Value = serde_json::from_str(line).map_err(|e| EmbedError::Failed(format!("request: {e}")))?;
    let passages = value.get("passages").and_then(Value::as_array).ok_or_else(|| EmbedError::Failed("request: no passages".into()))?;
    passages
        .iter()
        .map(|p| {
            let text = |key: &str| p.get(key).and_then(Value::as_str).map(str::to_string);
            match (text("query"), text("text")) {
                (Some(query), _) => Ok(Passage::Query(query)),
                (None, Some(text)) => Ok(Passage::Document { title: text_or_empty(p, "title"), text }),
                _ => Err(EmbedError::Failed(format!("request: a passage with no text: {p}"))),
            }
        })
        .collect()
}

fn text_or_empty(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

pub fn response(result: &Result<Vec<Vector>, EmbedError>) -> String {
    match result {
        Ok(vectors) => json!({"vectors": vectors.iter().map(|v| v.to_vec()).collect::<Vec<_>>()}).to_string(),
        Err(EmbedError::Unavailable(why) | EmbedError::Failed(why)) => json!({"error": why}).to_string(),
    }
}

pub fn read_response(line: &str) -> Result<Vec<Vector>, EmbedError> {
    let value: Value = serde_json::from_str(line).map_err(|e| EmbedError::Failed(format!("response: {e}")))?;
    if let Some(why) = value.get("error").and_then(Value::as_str) {
        return Err(EmbedError::Failed(why.to_string()));
    }
    let vectors = value.get("vectors").and_then(Value::as_array).ok_or_else(|| EmbedError::Failed("response: no vectors".into()))?;
    vectors
        .iter()
        .map(|v| {
            let bytes: Vec<i8> = v.as_array().into_iter().flatten().filter_map(Value::as_i64).filter_map(|b| i8::try_from(b).ok()).collect();
            <Vector>::try_from(bytes).map_err(|b| EmbedError::Failed(format!("response: a vector of {} bytes, not {DIMS}", b.len())))
        })
        .collect()
}
