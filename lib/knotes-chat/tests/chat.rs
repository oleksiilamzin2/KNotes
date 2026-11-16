mod common;

use std::cell::{Cell, RefCell};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use common::Fake;
use knotes_chat::{AccessToken, Chat, ChatError, ChatEvent, INSTRUCTIONS, Memory, Method, Model, Summary, ToolBox, compact_at, list_models};
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

/// One tool, `search`, answering with a note, keeping its calls; a vault with spaces when
/// `spaced`.
#[derive(Default)]
struct Tools {
    calls: RefCell<Vec<(String, Value)>>,
    spaced: Cell<bool>,
}

impl ToolBox for Tools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec { name: "search", description: "Search the vault", parameters: json!({"type": "object", "properties": {"query": {"type": "string"}}}) }]
    }

    fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError> {
        self.calls.borrow_mut().push((name.into(), args.clone()));
        Ok(json!({"results": [{"path": "Radar.md"}]}))
    }

    fn overview(&self) -> String {
        "The vault: 1 note and 0 PDFs.".into()
    }

    fn excluded(&self, path: &str) -> bool {
        path.starts_with("Private/")
    }

    fn spaces(&self) -> String {
        if self.spaced.get() { "The vault's spaces: Робота (Work), Тварини (Pets).".into() } else { String::new() }
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
    Chat { transport: fake, token, model: "gpt-5.5".into(), cache_key: "conversation-1".into(), retry_after: vec![Duration::ZERO, Duration::ZERO], compact_at: compact_at(None), today: "Saturday, 2026-11-14 (UTC+02:00)".into(), effort: None, space: None }
}

/// Sends `text` and gives the outcome with the events on the way.
fn send(chat: &Chat, items: &mut Vec<Value>, text: &str, open: Option<&str>, tools: &Tools) -> (Result<(), ChatError>, Vec<ChatEvent>) {
    let mut events = Vec::new();
    let result = chat.send(items, &mut Memory::default(), text, open, tools, &AtomicBool::new(false), &mut |e| events.push(e));
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
    let instructions = body["instructions"].as_str().unwrap();
    assert_eq!(instructions, INSTRUCTIONS, "nothing that changes goes in the instructions: they head every request's cached prefix");
    assert!(instructions.contains("Knotes") && instructions.contains("next_offset"), "{instructions}");
    assert!(instructions.contains("stem") && instructions.contains("пральна"), "the model searches by stems: {instructions}");
    assert!(instructions.contains("language the notes are written in") && instructions.contains("notes disagree") && instructions.contains("ask which note"), "{instructions}");
    assert!(instructions.contains("Search a date first as the user says it") && instructions.contains("2026-11-18") && instructions.contains("Prefer figures written in the notes"), "{instructions}");
    assert!(instructions.contains("it sees only checkbox items"), "{instructions}");
    assert!(instructions.contains("[[Projects/Radar#Budget]]") && instructions.contains("list_tasks") && instructions.contains("related"), "{instructions}");
    assert!(body.get("reasoning").is_none(), "no effort chosen: the model's own");
    for field in ["temperature", "max_output_tokens", "previous_response_id", "truncation", "metadata"] {
        assert!(body.get(field).is_none(), "{field} is not allowed on this route");
    }
    let namespace = &body["tools"][0];
    assert_eq!((&namespace["type"], &namespace["name"]), (&json!("namespace"), &json!("vault")));
    assert_eq!(namespace["tools"][0]["type"], "function");
    assert_eq!(namespace["tools"][0]["name"], "search");
    assert_eq!(body["input"][0], json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Today is Saturday, 2026-11-14 (UTC+02:00). The vault: 1 note and 0 PDFs."}]}), "the day and the vault, set when the conversation starts");
    assert_eq!(body["input"][1]["role"], "developer");
    assert!(body["input"][1]["content"][0]["text"].as_str().unwrap().contains("Projects/Radar.md"));
    assert_eq!(body["input"][2], json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}));
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
    assert_eq!(input[2], json!({"type": "reasoning", "summary": [], "encrypted_content": "sealed"}), "sent back without its id");
    assert_eq!(input[3], json!({"type": "function_call", "call_id": "call_1", "name": "search", "arguments": "{\"query\":\"radar\"}"}));
    assert_eq!(input[4]["type"], "function_call_output");
    assert_eq!(input[4]["call_id"], "call_1");
    assert!(input[4]["output"].as_str().unwrap().contains("Radar.md"));
    assert_eq!(items.len(), 5, "user, reasoning, call, output, answer: {items:?}");
}

#[test]
fn after_six_tool_calls_the_model_is_reminded_once_to_answer_what_has_not_turned_up() {
    let call = |i: usize| json!({"id": format!("fc_{i}"), "type": "function_call", "status": "completed", "call_id": format!("call_{i}"), "name": "search", "arguments": "{\"query\":\"x\"}"});
    let fake = Fake::answering(vec![reply((0..4).map(call).collect()), reply((4..7).map(call).collect()), reply((7..10).map(call).collect()), reply(vec![message("Not in the notes")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let (result, _) = send(&chat(&fake, &token), &mut items, "when is my gym?", None, &tools);
    assert_eq!(result, Ok(()));
    let reminders = |n: usize| fake.body(n)["input"].as_array().unwrap().iter().filter(|i| i["role"] == "developer" && i["content"][0]["text"].as_str().unwrap_or("").contains("tool calls so far")).count();
    assert_eq!(reminders(1), 0, "after four calls");
    assert_eq!(reminders(2), 1, "after seven");
    let input = fake.body(2)["input"].as_array().unwrap().clone();
    let last = input.last().unwrap()["content"][0]["text"].as_str().unwrap().to_string();
    assert!(last.starts_with("7 tool calls so far") && last.contains("does not have it") && last.contains("a list or a period"), "after the outputs: {last}");
    assert_eq!(reminders(3), 1, "once a turn");
}

#[test]
fn the_window_s_space_is_told_each_turn_before_the_question() {
    let fake = Fake::answering(vec![reply(vec![message("ok")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let chat = Chat { space: Some("Робота".into()), ..chat(&fake, &token) };
    let (result, _) = send(&chat, &mut items, "що в планах?", None, &tools);
    assert_eq!(result, Ok(()));
    let input = fake.body(0)["input"].as_array().unwrap().clone();
    let told = input[input.len() - 2]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert_eq!(input[input.len() - 2]["role"], "developer");
    assert!(told.contains("«Робота»") && told.contains("first"), "{told}");
    assert_eq!(fake.body(0)["instructions"], INSTRUCTIONS, "not in the instructions: their cached start stays");
}

#[test]
fn the_vault_s_spaces_are_told_each_turn_so_a_conversation_begun_before_them_knows_them() {
    let fake = Fake::answering(vec![reply(vec![message("ok")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    tools.spaced.set(true);
    let (result, _) = send(&chat(&fake, &token), &mut items, "які є простори?", None, &tools);
    assert_eq!(result, Ok(()));
    let input = fake.body(0)["input"].as_array().unwrap().clone();
    let told = input[input.len() - 2]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(told.contains("Робота (Work), Тварини (Pets)") && told.contains("whole vault"), "the window in no space: {told}");
    assert!(INSTRUCTIONS.contains("list_spaces") && INSTRUCTIONS.contains("профілі"), "the model knows what spaces are and their other names");
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
        // Without tries again: they have a test of their own.
        let once = Chat { retry_after: Vec::new(), ..chat(&fake, &token) };
        let (result, _) = send(&once, &mut items, "hi", None, &tools);
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
    let result = chat(&fake, &token).send(&mut items, &mut Memory::default(), "hi", None, &tools, &AtomicBool::new(true), &mut |_| {});
    assert_eq!(result, Err(ChatError::Cancelled));
    assert!(items.is_empty());
}

#[test]
fn the_models_offered_are_those_listed() {
    let models = json!({"models": [
        {"slug": "gpt-5.6-terra", "display_name": "GPT-5.6 Terra", "visibility": "list", "context_window": 272_000,
         "default_reasoning_level": "low",
         "supported_reasoning_levels": [{"effort": "low", "description": "Fast"}, {"effort": "medium", "description": "Balanced"}, {"effort": "xhigh", "description": "Deepest"}]},
        {"slug": "gpt-5.5", "display_name": "GPT-5.5", "visibility": "list"},
        {"slug": "internal", "display_name": "x", "visibility": "hide"},
    ]});
    let fake = Fake::answering(vec![(200, models.to_string())]);
    let token = Token::default();
    let levels = |l: &[&str]| l.iter().map(|e| e.to_string()).collect::<Vec<_>>();
    let terra = Model { slug: "gpt-5.6-terra".into(), name: "GPT-5.6 Terra".into(), context_window: Some(272_000), efforts: levels(&["low", "medium", "xhigh"]), default_effort: "low".into() };
    // A model the list says nothing of how it reasons gets the levels every reasoning model takes.
    let plain = Model { slug: "gpt-5.5".into(), name: "GPT-5.5".into(), context_window: None, efforts: levels(&["low", "medium", "high"]), default_effort: "medium".into() };
    assert_eq!(list_models(&fake, &token), Ok(vec![terra, plain]));
    let request = &fake.sent.borrow()[0];
    assert_eq!((request.method, request.url.as_str()), (Method::Get, "https://api.openai.com/v1/models"));
    assert_eq!(fake.header(0, "Authorization"), "Bearer old");
}

#[test]
fn a_moment_s_unavailability_or_network_failure_is_tried_again_but_a_usage_limit_is_not() {
    let unavailable = (503, json!({"error": {"code": "subscription_sharing_usage_unavailable", "message": "later"}}).to_string());
    let fake = Fake::answering(vec![unavailable.clone(), reply(vec![message("ok")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    assert_eq!(send(&chat(&fake, &token), &mut items, "hi", None, &tools).0, Ok(()));
    assert_eq!(fake.sent.borrow().len(), 2);

    let fake = Fake::default();
    fake.answers.borrow_mut().push_back(Err(ChatError::Network("offline".into())));
    fake.answers.borrow_mut().push_back(Ok(reply(vec![message("ok")])));
    assert_eq!(send(&chat(&fake, &token), &mut Vec::new(), "hi", None, &tools).0, Ok(()));

    let fake = Fake::answering(vec![unavailable.clone(), unavailable.clone(), unavailable]);
    assert_eq!(send(&chat(&fake, &token), &mut Vec::new(), "hi", None, &tools).0, Err(ChatError::Unavailable("later".into())), "two tries more, then it says so");
    assert_eq!(fake.sent.borrow().len(), 3);

    let limit = (429, json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "limit"}}).to_string());
    let fake = Fake::answering(vec![limit]);
    assert_eq!(send(&chat(&fake, &token), &mut Vec::new(), "hi", None, &tools).0, Err(ChatError::UsageLimit));
    assert_eq!(fake.sent.borrow().len(), 1);
}

/// A turn of a conversation: a question, a search with a long result, an answer.
fn long_turn(question: &str) -> Vec<serde_json::Value> {
    vec![
        json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": question}]}),
        json!({"type": "reasoning", "summary": [], "encrypted_content": "sealed"}),
        json!({"type": "function_call", "call_id": format!("c-{question}"), "name": "search", "arguments": "{}"}),
        json!({"type": "function_call_output", "call_id": format!("c-{question}"), "output": "нотатка ".repeat(20_000)}),
        json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": format!("answer to {question}")}]}),
    ]
}

#[test]
fn a_long_conversation_has_its_older_part_summarized_and_sends_the_summary_with_the_last_turns() {
    let mut items: Vec<serde_json::Value> = ["first?", "second?", "third?"].iter().flat_map(|q| long_turn(q)).collect();
    // Without the summary even the cut results would not matter: the uncut last turn alone is long.
    let fake = Fake::answering(vec![reply(vec![message("Asked about first and second; found nothing.")]), reply(vec![message("fourth answer")])]);
    let (token, tools) = (Token::default(), Tools::default());
    let mut memory = Memory::default();
    let mut events = Vec::new();
    let small = Chat { compact_at: 60_000, ..chat(&fake, &token) };
    let result = small.send(&mut items, &mut memory, "fourth?", None, &tools, &AtomicBool::new(false), &mut |e| events.push(e));
    assert_eq!(result, Ok(()));
    assert!(events.contains(&ChatEvent::Compacting), "{events:?}");
    assert_eq!(memory.summary, Some(Summary { covers: 10, text: "Asked about first and second; found nothing.".into() }), "the first two turns, the last two kept");

    let asked = fake.body(0);
    assert!(asked.get("tools").is_none(), "summarizing calls no tools");
    assert!(asked["instructions"].as_str().unwrap().contains("summarize"));
    let words = asked["input"][0]["content"][0]["text"].as_str().unwrap();
    assert!(words.contains("User: first?") && words.contains("Assistant: answer to second?") && !words.contains("third?"), "{}", &words[..300]);
    assert!(words.len() < 10_000, "tool results are cut in what is summarized: {}", words.len());

    let input = fake.body(1)["input"].as_array().unwrap().clone();
    assert!(input[0]["content"][0]["text"].as_str().unwrap().starts_with("Today is"), "{}", input[0]);
    assert!(input[1]["content"][0]["text"].as_str().unwrap().contains("Asked about first and second"), "{}", input[1]);
    assert_eq!(input[2]["content"][0]["text"], "third?");
    assert_eq!(input.last().unwrap()["content"][0]["text"], "fourth?");
    assert_eq!(items.len(), 17, "the conversation keeps every item; only what is sent changes");
}

#[test]
fn a_short_conversation_is_not_summarized() {
    let mut items: Vec<serde_json::Value> = long_turn("first?").into_iter().map(|mut i| {
        if i["type"] == "function_call_output" {
            i["output"] = json!("short");
        }
        i
    }).collect();
    let fake = Fake::answering(vec![reply(vec![message("ok")])]);
    let (token, tools) = (Token::default(), Tools::default());
    let mut memory = Memory::default();
    chat(&fake, &token).send(&mut items, &mut memory, "second?", None, &tools, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!((memory.summary, fake.sent.borrow().len()), (None, 1));
}

/// A reply whose request OpenAI counted as `input_tokens`.
fn counted(output: Vec<serde_json::Value>, input_tokens: u64) -> (u16, String) {
    let (status, body) = reply(output);
    let body = body.replace("\"response\":{\"id\":\"resp_1\",\"output\"", &format!("\"response\":{{\"id\":\"resp_1\",\"usage\":{{\"input_tokens\":{input_tokens},\"input_tokens_details\":{{\"cached_tokens\":{}}},\"output_tokens\":5}},\"output\"", input_tokens / 2));
    assert!(body.contains("input_tokens"), "{body}");
    (status, body)
}

#[test]
fn the_conversation_s_size_is_openai_s_count_and_past_the_threshold_it_is_summarized() {
    let (token, tools) = (Token::default(), Tools::default());
    let fake = Fake::answering(vec![counted(vec![message("ok")], 1_234)]);
    let mut items = Vec::new();
    let mut memory = Memory::default();
    chat(&fake, &token).send(&mut items, &mut memory, "hi", None, &tools, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let size = memory.tokens.unwrap();
    assert!((1_235..1_300).contains(&size), "the count of the request and the answer after it: {size}");

    // Small items, but OpenAI counted the conversation past 90 % of the window: summarized.
    let mut items: Vec<serde_json::Value> = ["first?", "second?"].iter().flat_map(|q| long_turn(q)).collect();
    let mut memory = Memory { tokens: Some(250_000), ..Memory::default() };
    let fake = Fake::answering(vec![reply(vec![message("Summary.")]), counted(vec![message("third answer")], 40_000)]);
    let mut events = Vec::new();
    chat(&fake, &token).send(&mut items, &mut memory, "third?", None, &tools, &AtomicBool::new(false), &mut |e| events.push(e)).unwrap();
    assert!(events.contains(&ChatEvent::Compacting), "{events:?}");
    assert_eq!(memory.summary.as_ref().map(|s| s.covers), Some(5));
    assert!(memory.tokens.is_some_and(|t| (40_000..40_100).contains(&t)), "{:?}", memory.tokens);
}

#[test]
fn the_threshold_is_ninety_per_cent_of_the_window_up_to_where_requests_cost_double() {
    assert_eq!(compact_at(None), 244_800);
    assert_eq!(compact_at(Some(1_050_000)), 244_800);
    assert_eq!(compact_at(Some(272_000)), 244_800);
    assert_eq!(compact_at(Some(128_000)), 115_200);
}

/// Tools whose vault overview changes each time it is asked for.
#[derive(Default)]
struct Changing(Cell<u32>);

impl ToolBox for Changing {
    fn specs(&self) -> Vec<ToolSpec> {
        Vec::new()
    }

    fn call(&self, _name: &str, _args: &Value) -> Result<Value, ToolError> {
        Ok(json!({}))
    }

    fn overview(&self) -> String {
        self.0.set(self.0.get() + 1);
        format!("The vault: {} notes.", self.0.get())
    }
}

#[test]
fn the_day_and_the_vault_are_set_once_a_conversation_so_its_cached_start_stays_the_same() {
    let fake = Fake::answering(vec![counted(vec![message("one")], 2_000), counted(vec![message("two")], 3_000)]);
    let (token, tools) = (Token::default(), Changing::default());
    let mut items = Vec::new();
    let mut memory = Memory::default();
    let chat = chat(&fake, &token);
    chat.send(&mut items, &mut memory, "first?", None, &tools, &AtomicBool::new(false), &mut |_| {}).unwrap();
    chat.send(&mut items, &mut memory, "second?", None, &tools, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let (first, second) = (fake.body(0)["input"].as_array().unwrap().clone(), fake.body(1)["input"].as_array().unwrap().clone());
    assert_eq!(first[0], second[0], "the vault changed between the turns; what the model was told did not");
    assert_eq!(&second[..first.len()], &first[..], "the second request starts with the first one, item for item");
    assert_eq!(memory.preamble.as_deref(), Some("Today is Saturday, 2026-11-14 (UTC+02:00). The vault: 1 notes."));
    assert_eq!(memory.cached, Some(1_500), "OpenAI's count of the cached part, kept to be seen");
}

#[test]
fn a_summary_brings_the_day_and_the_vault_up_to_date() {
    let mut items: Vec<serde_json::Value> = ["first?", "second?", "third?"].iter().flat_map(|q| long_turn(q)).collect();
    let fake = Fake::answering(vec![reply(vec![message("Summary.")]), reply(vec![message("ok")])]);
    let (token, tools) = (Token::default(), Changing::default());
    let mut memory = Memory { preamble: Some("Today is long ago.".into()), ..Memory::default() };
    let small = Chat { compact_at: 60_000, ..chat(&fake, &token) };
    small.send(&mut items, &mut memory, "fourth?", None, &tools, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(memory.preamble.as_deref(), Some("Today is Saturday, 2026-11-14 (UTC+02:00). The vault: 1 notes."));
    assert!(fake.body(1)["input"][0]["content"][0]["text"].as_str().unwrap().starts_with("Today is Saturday"));
}

#[test]
fn a_document_kept_from_the_ai_is_not_named_to_it() {
    let fake = Fake::answering(vec![reply(vec![message("ok")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    send(&chat(&fake, &token), &mut items, "what is here?", Some("Private/Secret.md"), &tools).0.unwrap();
    assert!(!fake.body(0).to_string().contains("Private/Secret.md"), "{}", fake.body(0)["input"]);
}

#[test]
fn the_effort_chosen_goes_with_each_request() {
    let fake = Fake::answering(vec![reply(vec![message("ok")])]);
    let (token, tools, mut items) = (Token::default(), Tools::default(), Vec::new());
    let chat = Chat { effort: Some("low".into()), ..chat(&fake, &token) };
    assert_eq!(send(&chat, &mut items, "hi", None, &tools).0, Ok(()));
    assert_eq!(fake.body(0)["reasoning"], json!({"effort": "low"}));
}

#[test]
fn a_note_kept_from_the_ai_after_it_was_read_is_not_sent_again() {
    // Read in an earlier turn, then tagged #no-ai (or its folder kept out): the conversation went
    // on sending what was read of it.
    let call = json!({"type": "function_call", "call_id": "call_1", "name": "read_note", "arguments": "{\"path\":\"Private/Secret.md\"}"});
    let read = json!({"type": "function_call_output", "call_id": "call_1", "output": json!({"path": "Private/Secret.md", "blocks": [{"text": "The secret code is 1234."}]}).to_string()});
    let mut items = vec![json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "the code?"}]}), call, read, message("It is 1234.")];
    let fake = Fake::answering(vec![reply(vec![message("Noted.")])]);
    let (token, tools) = (Token::default(), Tools::default());
    let (result, _) = send(&chat(&fake, &token), &mut items, "thanks", None, &tools);
    assert_eq!(result, Ok(()));
    let input = fake.body(0)["input"].to_string();
    assert!(!input.contains("The secret code"), "{input}");
    assert!(input.contains("withheld"), "{input}");
    assert!(items[2]["output"].as_str().unwrap().contains("The secret code"), "the conversation kept keeps it, for when it is shared again");
}
