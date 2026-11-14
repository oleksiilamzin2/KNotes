use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use crate::authorized::{error_of, send_authorized};
use crate::{API, AccessToken, ChatError, HttpRequest, Method, SseEvents, ToolBox, Transport};

/// What the model is told about itself and the tools.
pub const INSTRUCTIONS: &str = "You are the assistant in Knotes, a notes app. You answer questions about the user's vault: \
Markdown notes and PDFs. Use the vault tools to find and read what you need before answering; don't guess what the notes say. \
Search gives fragments; read a note's blocks or a PDF's pages for more. When you use a note or a PDF, link it as an Obsidian \
wiki link with its path, like [[Projects/Radar]] for Projects/Radar.md or [[Papers/budget.pdf#page=3]] for a PDF's page; the app \
makes them clickable. Answer in the language the user writes in. Be concise.";

/// The tools' namespace (the route takes function tools only in one).
const NAMESPACE: &str = "vault";
/// Replies with tool calls one turn may take before it gives up.
const MAX_STEPS: usize = 16;

/// What a turn shows on the way.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// Text of the reply, a piece at a time.
    Text(String),
    /// The model calls a tool.
    ToolCall { name: String, arguments: Value },
    ToolDone { name: String, failed: bool },
}

/// One conversation's requests: the model, and a key that lets OpenAI reuse the
/// conversation's start from its cache.
pub struct Chat<'a> {
    pub transport: &'a dyn Transport,
    pub token: &'a dyn AccessToken,
    pub model: String,
    pub cache_key: String,
}

impl Chat<'_> {
    /// Sends the user's `text` (with the document they have open) after the conversation's
    /// `items`, runs the tools the model calls until it answers, and adds it all to `items`.
    /// A turn that fails or is called off leaves `items` as they were.
    pub fn send(&self, items: &mut Vec<Value>, text: &str, open: Option<&str>, tools: &dyn ToolBox, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<(), ChatError> {
        let before = items.len();
        let result = self.turn(items, text, open, tools, cancel, on_event);
        if result.is_err() {
            items.truncate(before);
        }
        result
    }

    fn turn(&self, items: &mut Vec<Value>, text: &str, open: Option<&str>, tools: &dyn ToolBox, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<(), ChatError> {
        if let Some(path) = open {
            let words = format!("The user has {path} open in Knotes; \"this note\", \"here\" and the like mean it.");
            items.push(json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": words}]}));
        }
        items.push(json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}));
        for _ in 0..MAX_STEPS {
            let output = self.respond(items, tools, cancel, on_event)?;
            let calls: Vec<Value> = output.iter().filter(|i| i["type"] == "function_call").cloned().collect();
            items.extend(output.iter().filter_map(kept));
            if calls.is_empty() {
                return Ok(());
            }
            for call in calls {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ChatError::Cancelled);
                }
                let name = call["name"].as_str().unwrap_or_default();
                let name = name.rsplit_once('.').map_or(name, |(_, n)| n).to_string();
                let output = match serde_json::from_str::<Value>(call["arguments"].as_str().unwrap_or("{}")) {
                    Ok(arguments) => {
                        on_event(ChatEvent::ToolCall { name: name.clone(), arguments: arguments.clone() });
                        tools.call(&name, &arguments).map_err(|e| e.to_string())
                    }
                    Err(e) => Err(format!("arguments are not JSON: {e}")),
                };
                on_event(ChatEvent::ToolDone { name, failed: output.is_err() });
                let output = output.unwrap_or_else(|e| json!({"error": e})).to_string();
                items.push(json!({"type": "function_call_output", "call_id": call["call_id"], "output": output}));
            }
        }
        Err(ChatError::TooManySteps)
    }

    /// One streamed reply to `items`: its output items once it says it is complete.
    fn respond(&self, items: &[Value], tools: &dyn ToolBox, cancel: &AtomicBool, on_event: &mut dyn FnMut(ChatEvent)) -> Result<Vec<Value>, ChatError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(ChatError::Cancelled);
        }
        let functions: Vec<Value> = tools
            .specs()
            .into_iter()
            .map(|s| json!({"type": "function", "name": s.name, "description": s.description, "parameters": s.parameters, "strict": false}))
            .collect();
        let body = json!({
            "model": self.model,
            "instructions": INSTRUCTIONS,
            "input": items,
            "tools": [{"type": "namespace", "name": NAMESPACE, "description": "Read the user's Knotes vault.", "tools": functions}],
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "include": ["reasoning.encrypted_content"],
            "prompt_cache_key": self.cache_key,
            "store": false,
            "stream": true,
        });
        let request = HttpRequest {
            method: Method::Post,
            url: format!("{API}/responses"),
            headers: vec![("Content-Type".into(), "application/json".into()), ("Accept".into(), "text/event-stream".into())],
            body: body.to_string().into_bytes(),
        };
        let response = send_authorized(self.transport, self.token, request)?;
        if !(200..300).contains(&response.status) {
            return Err(error_of(response));
        }
        let mut done = Vec::new();
        for event in SseEvents::new(response.body) {
            if cancel.load(Ordering::Relaxed) {
                return Err(ChatError::Cancelled);
            }
            let event = event.map_err(|e| ChatError::Network(e.to_string()))?;
            let Ok(data) = serde_json::from_str::<Value>(&event.data) else { continue };
            match data.get("type").and_then(Value::as_str).unwrap_or(&event.event) {
                "response.output_text.delta" => on_event(ChatEvent::Text(data["delta"].as_str().unwrap_or_default().to_string())),
                "response.output_item.done" => done.push(data["item"].clone()),
                "response.completed" => {
                    let output = data["response"]["output"].as_array().filter(|o| !o.is_empty()).cloned();
                    return Ok(output.unwrap_or(done));
                }
                "response.failed" => return Err(ChatError::from_openai(&data["response"]["error"], 0)),
                "response.incomplete" => {
                    let reason = data["response"]["incomplete_details"]["reason"].as_str().unwrap_or("unknown");
                    return Err(ChatError::Incomplete(reason.to_string()));
                }
                "error" => return Err(ChatError::from_openai(data.get("error").filter(|e| e.is_object()).unwrap_or(&data), 0)),
                _ => {}
            }
        }
        Err(ChatError::Interrupted)
    }
}

/// An output item as the conversation keeps it to send back: without the ids the server
/// keeps none of (`store: false`), a message as its text, reasoning only when sealed for
/// sending back; other items dropped.
fn kept(item: &Value) -> Option<Value> {
    match item["type"].as_str()? {
        "message" => {
            let text: String = item["content"].as_array()?.iter().filter_map(|c| c["text"].as_str()).collect();
            Some(json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}))
        }
        "reasoning" if item.get("encrypted_content").is_some_and(Value::is_string) => Some(without_ids(item)),
        "function_call" => Some(without_ids(item)),
        _ => None,
    }
}

fn without_ids(item: &Value) -> Value {
    let mut item = item.clone();
    if let Some(fields) = item.as_object_mut() {
        fields.remove("id");
        fields.remove("status");
    }
    item
}
