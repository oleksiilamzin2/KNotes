use knotes_chat::{Summary, context, estimate_tokens};
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
fn only_the_turn_under_way_keeps_its_reasoning_and_only_the_last_two_their_whole_results() {
    let long = "x".repeat(5_000);
    let items: Vec<Value> = [turn("one", &long), turn("two", &long), turn("three", &long)].concat();
    let sent = context(&items, None);
    let reasoning: Vec<&Value> = sent.iter().filter(|i| i["type"] == "reasoning").collect();
    assert_eq!(reasoning, vec![&json!({"type": "reasoning", "encrypted_content": "sealed three"})]);
    let outputs: Vec<usize> = sent.iter().filter(|i| i["type"] == "function_call_output").map(|i| i["output"].as_str().unwrap().len()).collect();
    assert!(outputs[0] < 2_000 && outputs[1] == 5_000 && outputs[2] == 5_000, "{outputs:?}");
    assert!(sent.iter().any(|i| i["output"].as_str().is_some_and(|o| o.ends_with("call the tool again for the rest]"))));
    assert_eq!(sent.len(), items.len() - 2, "two old reasoning items left out, nothing else");
}

#[test]
fn a_summary_stands_in_for_the_items_it_covers() {
    let items: Vec<Value> = [turn("one", "r"), turn("two", "r"), turn("three", "r")].concat();
    let summary = Summary { covers: 6, text: "Asked one.".into() };
    let sent = context(&items, Some(&summary));
    assert_eq!(sent[0]["role"], "developer");
    assert!(sent[0]["content"][0]["text"].as_str().unwrap().ends_with("Asked one."));
    assert_eq!(sent[2], user("two"));
    assert!(!sent.iter().any(|i| *i == user("one")));
}

#[test]
fn tokens_are_estimated_from_the_json_s_size() {
    let input = vec![user(&"слово ".repeat(1000))];
    let estimate = estimate_tokens(&input);
    assert!((2_700..2_900).contains(&estimate), "11 bytes a word, over four: {estimate}");
}
