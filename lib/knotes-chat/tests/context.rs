use knotes_chat::{Memory, Summary, context, estimate_tokens};
use serde_json::{Value, json};

fn user(text: &str) -> Value {
    json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]})
}

fn turn(question: &str, result: &str) -> Vec<Value> {
    vec![
        json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "The user has Radar.md open"}]}),
        user(question),
        json!({"type": "reasoning", "encrypted_content": format!("sealed {question}")}),
        json!({"type": "function_call", "call_id": question, "name": "search", "arguments": "{}"}),
        json!({"type": "function_call_output", "call_id": question, "output": result}),
        json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "answer"}]}),
    ]
}

#[test]
fn where_openai_caches_a_conversation_it_is_sent_as_it_is_kept() {
    let long = "x".repeat(5_000);
    let items: Vec<Value> = [turn("one", &long), turn("two", &long), turn("three", &long)].concat();
    let caching = Memory { cached: Some(4_000), ..Memory::default() };
    assert_eq!(context(&items, &caching), items, "nothing cut, no reasoning left out: each request starts as the one before did, so the cache covers it");
}

#[test]
fn where_nothing_is_cached_old_results_are_cut_and_past_reasoning_left_out() {
    let long = "x".repeat(5_000);
    let items: Vec<Value> = [turn("one", &long), turn("two", &long), turn("three", &long)].concat();
    for memory in [Memory::default(), Memory { cached: Some(0), ..Memory::default() }] {
        let sent = context(&items, &memory);
        let reasoning: Vec<&Value> = sent.iter().filter(|i| i["type"] == "reasoning").collect();
        assert_eq!(reasoning, vec![&json!({"type": "reasoning", "encrypted_content": "sealed three"})], "only the turn under way's");
        let outputs: Vec<usize> = sent.iter().filter(|i| i["type"] == "function_call_output").map(|i| i["output"].as_str().unwrap().len()).collect();
        assert!(outputs[0] < 2_000 && outputs[1] == 5_000 && outputs[2] == 5_000, "{outputs:?}");
    }
}

#[test]
fn the_day_and_the_vault_come_first_then_the_summary_in_place_of_what_it_covers() {
    let items: Vec<Value> = [turn("one", "r"), turn("two", "r"), turn("three", "r")].concat();
    let memory = Memory { preamble: Some("Today is Saturday.".into()), summary: Some(Summary { covers: 6, text: "Asked one.".into() }), cached: Some(100), ..Memory::default() };
    let sent = context(&items, &memory);
    assert_eq!(sent[0], json!({"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Today is Saturday."}]}));
    assert!(sent[1]["content"][0]["text"].as_str().unwrap().ends_with("Asked one."));
    assert_eq!(sent[3], user("two"));
    assert!(!sent.iter().any(|i| *i == user("one")));
    assert_eq!(sent.len(), 2 + items.len() - 6);
}

#[test]
fn tokens_are_estimated_from_the_json_s_size() {
    let input = vec![user(&"слово ".repeat(1000))];
    let estimate = estimate_tokens(&input);
    assert!((2_700..2_900).contains(&estimate), "11 bytes a word, over four: {estimate}");
}
