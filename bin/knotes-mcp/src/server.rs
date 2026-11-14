//! MCP over JSON-RPC 2.0: one message a line in, one answer a line out (stdio transport).

use knotes_core::{DocumentPages, VaultReader};
use knotes_tools::{ToolIndex, VaultTools};
use serde_json::{Value, json};

/// Protocol versions answered as asked; any other gets the first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const NO_METHOD: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Answers MCP messages with the vault's tools.
pub struct McpServer<I, F, D> {
    tools: VaultTools<I, F, D>,
}

impl<I: ToolIndex, F: VaultReader, D: DocumentPages> McpServer<I, F, D> {
    pub fn new(tools: VaultTools<I, F, D>) -> Self {
        Self { tools }
    }

    /// The answer to one line, none for a notification.
    pub fn answer_line(&self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(message) => self.answer(&message),
            Err(e) => Some(error(&Value::Null, PARSE_ERROR, &e.to_string())),
        }
    }

    fn answer(&self, message: &Value) -> Option<Value> {
        let id = message.get("id")?;
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(error(id, INVALID_REQUEST, "no method"));
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        Some(match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
                let version = VERSIONS.into_iter().find(|v| *v == asked).unwrap_or(VERSIONS[0]);
                result(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": "knotes", "title": "Knotes", "version": env!("CARGO_PKG_VERSION")},
                        "instructions": "Read-only tools over the user's Knotes vault (Markdown notes and PDFs): search it, read notes by block and PDFs by page, follow links."
                    }),
                )
            }
            "ping" => result(id, json!({})),
            "tools/list" => {
                let tools: Vec<Value> = VaultTools::<I, F, D>::specs()
                    .into_iter()
                    .map(|s| json!({"name": s.name, "description": s.description, "inputSchema": s.parameters}))
                    .collect();
                result(id, json!({"tools": tools}))
            }
            "tools/call" => match params.get("name").and_then(Value::as_str) {
                Some(name) => {
                    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                    let (text, failed) = match self.tools.call(name, &args) {
                        Ok(value) => (value.to_string(), false),
                        Err(e) => (e.to_string(), true),
                    };
                    result(id, json!({"content": [{"type": "text", "text": text}], "isError": failed}))
                }
                None => error(id, INVALID_PARAMS, "no tool name"),
            },
            _ => error(id, NO_METHOD, &format!("no method {method}")),
        })
    }
}

fn result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
