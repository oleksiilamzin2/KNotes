use knotes_chat::{Conversation, Conversations, Memory, Shown, Summary};
use serde_json::json;

fn talked(model: &str, question: &str) -> Conversation {
    let mut conversation = Conversation::new(model);
    conversation.items = vec![
        json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "The user has Radar.md open"}]}),
        json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": question}]}),
        json!({"type": "reasoning", "summary": [], "encrypted_content": "sealed"}),
        json!({"type": "function_call", "call_id": "c1", "name": "search", "arguments": "{\"query\":\"radar\"}"}),
        json!({"type": "function_call_output", "call_id": "c1", "output": "{}"}),
        json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "In [[Radar]]."}]}),
    ];
    conversation.name_from_first_question();
    conversation
}

#[test]
fn a_conversation_shows_its_questions_answers_and_tool_calls() {
    let conversation = talked("gpt-5.5", "Where is radar?");
    assert_eq!(
        conversation.shown(),
        vec![Shown::User("Where is radar?".into()), Shown::Tool { name: "search".into(), arguments: json!({"query": "radar"}) }, Shown::Assistant("In [[Radar]].".into())]
    );
}

#[test]
fn a_conversation_is_named_by_its_first_question_cut_short() {
    assert_eq!(talked("m", "Where is radar?\nAnd Boris?").title, "Where is radar?");
    let long = "word ".repeat(30);
    let title = talked("m", &long).title;
    assert!(title.chars().count() <= 61 && title.ends_with('…'), "{title}");
    assert_eq!(Conversation::new("m").title, "");
}

#[test]
fn conversations_are_kept_listed_newest_first_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let store = Conversations::new(dir.path().join("conversations"));
    assert!(store.list().is_empty());
    let mut first = talked("gpt-5.5", "First?");
    first.updated = 100;
    let mut second = talked("gpt-5.5", "Second?");
    second.updated = 200;
    second.memory = Memory { summary: Some(Summary { covers: 4, text: "Asked about radar.".into() }), tokens: Some(12_345) };
    assert_ne!(first.id, second.id);
    store.save(&first).unwrap();
    store.save(&second).unwrap();
    let titles: Vec<String> = store.list().into_iter().map(|e| e.title).collect();
    assert_eq!(titles, ["Second?", "First?"]);
    assert_eq!(store.load(&first.id), Some(first.clone()));
    assert_eq!(store.load(&second.id), Some(second.clone()), "its summary and size kept too");

    first.updated = 300;
    first.title = "First, renamed".into();
    store.save(&first).unwrap();
    let again = Conversations::new(dir.path().join("conversations"));
    let titles: Vec<String> = again.list().into_iter().map(|e| e.title).collect();
    assert_eq!(titles, ["First, renamed", "Second?"]);

    again.delete(&first.id);
    assert_eq!(again.list().len(), 1);
    assert_eq!(again.load(&first.id), None);
    assert_eq!(again.load("../escape"), None);
}
