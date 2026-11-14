mod common;

use std::cell::{Cell, RefCell};
use std::sync::atomic::AtomicBool;

use common::Fake;
use knotes_chat::{AccessToken, Chat, ChatError, ChatEvent, Method, Model, ToolBox, list_models};
use knotes_tools::{ToolError, ToolSpec};
use serde_json::{Value, json};

/// "old" until renewed, then "new"; renewing twice fails.
#[derive(Default)]
struct Token {
    renewed: Cell<u32>,
}

impl AccessToken for Token {
    fn token(&self) -> Result<String, ChatError> {
        Ok(if self.renewed.get() == 0 { "old" } else { "new" }.into())
    }

    fn renew(&self) -> Result<String, ChatError> {
        self.renewed.set(self.renewed.get() + 1);
        if self.renewed.get() > 1 { Err(ChatError::SignInAgain("refresh token reused".into())) } else { Ok("new".into()) }
    }
}

/// One tool, `search`, answering with a note, keeping its calls.
#[derive(Default)]
struct Tools {
    calls: RefCell<Vec<(String, Value)>>,
}

impl ToolBox for Tools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec { name: "search", description: "Search the vault", parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}}) }]
    }

    fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError> {
        self.calls.borrow_mut().push((name.into(), args.clone()));
        Ok(json!({"results": [{"path": "Radar.md"}]}))
    }
}

/// A stream of these events, as OpenAI sends them.
fn sse(events: &[Value]) -> String {
    events.iter().map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap())).collect()
}

fn message(text: &str) -> Value {
    json!({"id": "msg_1", "type": "message", "status": "completed", "role": "assistant", "content": [{"type": "output_text", "text": text, "annotations": []}]})
}

/// A whole streamed reply with `output`, its text in two deltas.
fn reply(output: Vec<Value>) -> (u16, String) {
    let mut events = vec![json!({"type": "response.created", "response": {"id": "resp_1"}})];
    for item in &output {
        if let Some(text) = item["content"][0]["text"].as_str() {
            let (a, b) = text.split_at(text.len() / 2);
            events.push(json!({"type": "response.output_text.delta", "delta": a}));
            events.push(json!({"type": "response.output_text.delta", "delta": b}));
        }
        events.push(json!({"type": "response.output_item.done", "item": item}));
    }
    events.push(json!({"type": "response.completed", "response": {"id": "resp_1", "output": output}}));
    (200, sse(&events))
}

fn chat<'a>(fake: &'a Fake, token: &'a Token) -> Chat<'a> {
    Chat { transport: fake, token, model: "gpt-5.5".into(), cache_key: "conversation-1".into() }
}

/// Sends `text` and gives the outcome with the events on the way.
fn send(chat: &Chat, items: &mut Vec<Value>, text: &str, open: Option<&str>, tools: &Tools) -> (Result<(), ChatError>, Vec<ChatEvent>) {
    let mut events = Vec::new();
    let result = chat.send(items, text, open, tools, &AtomicBool::new(false), &mut |e| events.push(e));
    (result, events)
}

fn texts(events: &[ChatEvent]) -> String {
    events.iter().filter_map(|e| if let ChatEvent::Text(t) = e { Some(t.as_str()) } else { None }).collect()
}

#[test]
fn a_reply_streams_its_text_and_joins_the_conversation() {
    let fake = Fake::answering(vec![reply(vec![message("Hello there")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let (result, events) = send(&chat(&fake, &token), &mut items, "hi", Some("Projects/Radar.md"), &tools);
    assert_eq!(result, Ok(()));
    assert_eq!(texts(&events), "Hello there");
    let request = &fake.sent.borrow()[0];
    assert_eq!((request.method, request.url.as_str()), (Method::Post, "https://api.openai.com/v1/responses"));
    assert_eq!(fake.header(0, "Authorization"), "Bearer old");
    let body = fake.body(0);
    assert_eq!((&body["store"], &body["stream"], &body["model"]), (&json!(false), &json!(true), &json!("gpt-5.5")));
    assert_eq!(body["prompt_cache_key"], "conversation-1");
    assert!(body["instructions"].as_str().unwrap().contains("Knotes"));
    for field in ["temperature", "max_output_tokens", "previous_response_id", "truncation", "metadata"] {
        assert!(body.get(field).is_none(), "{field} is not allowed on this route");
    }
    let namespace = &body["tools"][0];
    assert_eq!((&namespace["type"], &namespace["name"]), (&json!("namespace"), &json!("vault")));
    assert_eq!(namespace["tools"][0]["type"], "function");
    assert_eq!(namespace["tools"][0]["name"], "search");
    assert_eq!(body["input"][0]["role"], "developer");
    assert!(body["input"][0]["content"][0]["text"].as_str().unwrap().contains("Projects/Radar.md"));
    assert_eq!(body["input"][1], json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}));
    assert_eq!(items.len(), 3);
    assert_eq!(items[2], json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Hello there"}]}));
}

#[test]
fn a_tool_call_is_run_and_its_output_sent_back_with_the_reasoning() {
    let reasoning = json!({"id": "rs_1", "type": "reasoning", "summary": [], "encrypted_content": "sealed"});
    let call = json!({"id": "fc_1", "type": "function_call", "status": "completed", "call_id": "call_1", "name": "search", "arguments": "{\"query\":\"radar\"}"});
    let fake = Fake::answering(vec![reply(vec![reasoning, call]), reply(vec![message("Radar is in Radar.md")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let (result, events) = send(&chat(&fake, &token), &mut items, "where is radar?", None, &tools);
    assert_eq!(result, Ok(()));
    assert_eq!(*tools.calls.borrow(), vec![("search".to_string(), json!({"query": "radar"}))]);
    assert!(events.contains(&ChatEvent::ToolCall { name: "search".into(), arguments: json!({"query": "radar"}) }), "{events:?}");
    assert!(events.contains(&ChatEvent::ToolDone { name: "search".into(), failed: false }), "{events:?}");
    assert_eq!(texts(&events), "Radar is in Radar.md");
    let first = fake.body(0);
    assert!(first["include"].as_array().unwrap().contains(&json!("reasoning.encrypted_content")));
    let input = fake.body(1)["input"].as_array().unwrap().clone();
    assert_eq!(input[1], json!({"type": "reasoning", "summary": [], "encrypted_content": "sealed"}), "sent back without its id");
    assert_eq!(input[2], json!({"type": "function_call", "call_id": "call_1", "name": "search", "arguments": "{\"query\":\"radar\"}"}));
    assert_eq!(input[3]["type"], "function_call_output");
    assert_eq!(input[3]["call_id"], "call_1");
    assert!(input[3]["output"].as_str().unwrap().contains("Radar.md"));
    assert_eq!(items.len(), 5, "user, reasoning, call, output, answer: {items:?}");
}

#[test]
fn plan_errors_come_out_as_what_to_do_about_them() {
    let failed = sse(&[json!({"type": "response.failed", "response": {"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "limit"}}})]);
    let error = |status: u16, code: &str| (status, json!({"error": {"code": code, "message": "words", "param": "tools[0]"}}).to_string());
    let cases = [
        ((200, failed), ChatError::UsageLimit),
        (error(429, "subscription_sharing_usage_limit_exceeded"), ChatError::UsageLimit),
        (error(503, "subscription_sharing_usage_unavailable"), ChatError::Unavailable("words".into())),
        (error(503, "subscription_sharing_user_unavailable"), ChatError::Unavailable("words".into())),
        (error(400, "subscription_sharing_unsupported_capability"), ChatError::Unsupported("tools[0]".into())),
        (error(403, "subscription_sharing_user_not_eligible"), ChatError::NotEligible("words".into())),
        (error(400, "invalid_value"), ChatError::Failed("invalid_value: words".into())),
    ];
    for (answer, expected) in cases {
        let fake = Fake::answering(vec![answer]);
        let (token, tools) = (Token::default(), Tools::default());
        let mut items = vec![json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "before"}]})];
        let (result, _) = send(&chat(&fake, &token), &mut items, "hi", None, &tools);
        assert_eq!(result, Err(expected));
        assert_eq!(items.len(), 1, "a failed turn leaves the conversation as it was");
    }
}

#[test]
fn an_expired_token_is_renewed_once_then_sign_in_is_asked() {
    let fake = Fake::answering(vec![(401, "{}".into()), reply(vec![message("ok")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let (result, _) = send(&chat(&fake, &token), &mut items, "hi", None, &tools);
    assert_eq!(result, Ok(()));
    assert_eq!(fake.header(1, "Authorization"), "Bearer new");

    let invalid = json!({"error": {"code": "subscription_sharing_invalid_user", "message": "revoked"}}).to_string();
    let fake = Fake::answering(vec![(401, invalid.clone()), (401, invalid)]);
    let token = Token::default();
    let (result, _) = send(&chat(&fake, &token), &mut Vec::new(), "hi", None, &tools);
    assert_eq!(result, Err(ChatError::SignInAgain("revoked".into())));
}

#[test]
fn a_stream_cut_short_or_called_off_leaves_the_conversation_as_it_was() {
    let cut = sse(&[json!({"type": "response.output_text.delta", "delta": "Hal"})]);
    let fake = Fake::answering(vec![(200, cut)]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let (result, _) = send(&chat(&fake, &token), &mut items, "hi", None, &tools);
    assert_eq!(result, Err(ChatError::Interrupted));
    assert!(items.is_empty());

    let fake = Fake::answering(vec![reply(vec![message("never")])]);
    let result = chat(&fake, &token).send(&mut items, "hi", None, &tools, &AtomicBool::new(true), &mut |_| {});
    assert_eq!(result, Err(ChatError::Cancelled));
    assert!(items.is_empty());
}

#[test]
fn the_models_offered_are_those_listed() {
    let models = json!({"models": [{"slug": "gpt-5.5", "display_name": "GPT-5.5", "visibility": "list"}, {"slug": "internal", "display_name": "x", "visibility": "hide"}]});
    let fake = Fake::answering(vec![(200, models.to_string())]);
    let token = Token::default();
    assert_eq!(list_models(&fake, &token), Ok(vec![Model { slug: "gpt-5.5".into(), name: "GPT-5.5".into() }]));
    let request = &fake.sent.borrow()[0];
    assert_eq!((request.method, request.url.as_str()), (Method::Get, "https://api.openai.com/v1/models"));
    assert_eq!(fake.header(0, "Authorization"), "Bearer old");
}
