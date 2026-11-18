use std::sync::atomic::Ordering;

use knotes_assistant::chat_state::{ChatState, compact_at_for, effort_for};
use knotes_chat::{ChatError, ChatEvent, Conversations, Memory, Model, Registration};
use serde_json::json;

fn model(slug: &str, efforts: &[&str], default: &str, window: Option<u64>) -> Model {
    Model { slug: slug.into(), name: slug.to_uppercase(), context_window: window, efforts: efforts.iter().map(|e| (*e).to_owned()).collect(), default_effort: default.into() }
}

fn models() -> Vec<Model> {
    vec![model("big", &["low", "medium", "high"], "medium", Some(100_000)), model("small", &["low"], "low", None)]
}

fn state(model: &str, effort: Option<&str>) -> ChatState {
    ChatState::new(Registration::default(), model.into(), effort.map(str::to_owned))
}

/// A turn begun with `question`, whose words back go nowhere.
fn ask(state: &mut ChatState, question: &str) -> knotes_assistant::chat_worker::Turn {
    state.begin_turn(question.into(), Box::new(|_| {}), Box::new(|_, _, _| {}))
}

/// The items of a conversation that asked `question` and was answered.
fn answered(question: &str) -> Vec<serde_json::Value> {
    vec![json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": question}]}), json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Так."}]})]
}

#[test]
fn the_model_and_the_effort_chosen_stay_while_the_plan_has_them() {
    // Before the plan's list is in, what the last launch chose is asked for as it is.
    let mut chat = state("big", Some("high"));
    assert_eq!((chat.asked_effort().as_deref(), chat.efforts(), chat.model_name().as_str()), (Some("high"), None, "big"));
    chat.set_models(models());
    let (efforts, chosen) = chat.efforts().unwrap();
    assert_eq!((efforts.len(), chosen, chat.model_name().as_str(), chat.asked_effort().as_deref()), (3, Some(2), "BIG", Some("high")));
    assert_eq!(chat.compact_at(), 90_000, "nine tenths of the model's window");
    // Another model that does not take the effort: its own default instead, and none asked for.
    assert_eq!(chat.choose_model(1).as_deref(), Some("small"));
    assert_eq!((chat.asked_effort(), chat.used_effort().as_deref(), chat.efforts().unwrap().1), (None, Some("low"), Some(0)));
    assert_eq!(chat.choose_model(7), None);
    assert_eq!(chat.choose_model(-1), None);
    assert_eq!(chat.model, "small", "a row that is not there chooses nothing");
    // An effort of the model's own list is chosen by its place.
    chat.choose_model(0);
    assert_eq!((chat.choose_effort(0).as_deref(), chat.asked_effort().as_deref()), (Some("low"), Some("low")));
    assert_eq!(chat.choose_effort(9), None);
    // A model the plan has no more: the plan's first.
    let mut gone = state("retired", Some("high"));
    gone.set_models(models());
    assert_eq!((gone.model.as_str(), gone.asked_effort().as_deref()), ("big", Some("high")));
    let mut none = state("retired", None);
    none.set_models(Vec::new());
    assert_eq!((none.model.as_str(), none.efforts(), none.used_effort()), ("", None, None));
    assert_eq!(effort_for(&models(), "small", Some(&"high".to_owned())), None);
    assert!(compact_at_for(&models(), "small") > 90_000, "a model with no window said: the widest priced");
}

#[test]
fn a_turn_takes_what_the_window_offers_and_ends_saved_or_with_its_question_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut chat = state("big", None);
    chat.set_models(models());
    chat.conversations = Some(Conversations::new(dir.path().to_path_buf()));
    chat.space = Some(("work".into(), "Робота".into()));
    assert!(chat.set_document(Some("Plan.md".into())) && !chat.set_document(Some("Plan.md".into())), "the same document is no news");
    assert!(!chat.stop_turn() && !chat.chat_event(ChatEvent::Text("late".into())), "no turn runs");
    // The question goes with the document offered, the space and the model.
    let turn = ask(&mut chat, "Що в плані?");
    assert_eq!((turn.text.as_str(), turn.open.as_deref(), turn.space.clone(), turn.model.as_str(), turn.effort.clone(), turn.compact_at), ("Що в плані?", Some("Plan.md"), Some(("work".into(), "Робота".into())), "big", None, 90_000));
    assert_eq!(turn.cache_key, chat.current.id);
    // What it says is kept as it comes.
    assert!(chat.chat_event(ChatEvent::ToolCall { name: "search".into(), arguments: json!({"q": "план"}) }));
    chat.chat_event(ChatEvent::ToolDone { name: "search".into(), failed: false });
    chat.chat_event(ChatEvent::Compacting);
    chat.chat_event(ChatEvent::Text("Та".into()));
    chat.chat_event(ChatEvent::Text("к.".into()));
    let asking = chat.turn.as_ref().unwrap();
    assert_eq!((asking.reply.as_str(), asking.tools.len(), asking.compacting), ("Так.", 1, true));
    // No new conversation, and none deleted, while it runs; told to stop, it is.
    assert!(!chat.new_conversation() && !chat.delete_conversation());
    assert!(chat.stop_turn() && turn.cancel.load(Ordering::Relaxed));
    // Done well: the conversation takes the items, its first question's name, and is saved.
    let memory = Memory { tokens: Some(1_200), ..Memory::default() };
    assert_eq!(chat.turn_done(answered("Що в плані?"), memory.clone(), Ok(()), 1_800_000_000), None);
    assert!(chat.turn.is_none() && chat.error.is_none());
    assert_eq!((chat.current.title.as_str(), chat.current.updated, chat.entries.len(), chat.context()), ("Що в плані?", 1_800_000_000, 1, Some((1_200, 90_000))));
    // The document crossed out is not offered.
    chat.document_on = false;
    let turn = ask(&mut chat, "А далі?");
    assert_eq!((turn.open, turn.items.len()), (None, 2));
    // Failed: the error shows, the question comes back, the items stay as they were; a summary
    // made meanwhile is kept and saved, as it covers items from before the turn.
    let summed = Memory { tokens: Some(300), preamble: Some("Досі: план.".into()), ..Memory::default() };
    let back = chat.turn_done(Vec::new(), summed.clone(), Err(ChatError::Unavailable("offline".into())), 1_800_000_100);
    assert_eq!((back.as_deref(), chat.current.items.len(), chat.current.updated), (Some("А далі?"), 2, 1_800_000_000));
    assert!(matches!(chat.error, Some(ChatError::Unavailable(_))));
    let saved = Conversations::new(dir.path().to_path_buf()).load(&chat.current.id).unwrap();
    assert_eq!((saved.memory, saved.items.len()), (summed, 2));
    // The next turn clears the error.
    ask(&mut chat, "Ще раз");
    assert!(chat.error.is_none());
}

#[test]
fn conversations_are_listed_the_new_one_first_opened_begun_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let mut chat = state("big", None);
    chat.conversations = Some(Conversations::new(dir.path().to_path_buf()));
    let titles = |chat: &ChatState| chat.conversation_titles("Нова", "Без назви");
    assert_eq!(titles(&chat), (vec!["Нова".to_owned()], 0));
    assert!(!chat.new_conversation(), "the one open is new still");
    assert!(!chat.choose_conversation(0), "the new one's own row opens nothing");
    // Two conversations had: the one open is in the list, no new one above it.
    ask(&mut chat, "Перше питання");
    chat.turn_done(answered("Перше питання"), Memory::default(), Ok(()), 100);
    let first = chat.current.id.clone();
    assert!(chat.new_conversation());
    assert_eq!(titles(&chat), (vec!["Нова".to_owned(), "Перше питання".to_owned()], 0));
    ask(&mut chat, "Друге питання");
    chat.turn_done(answered("Друге питання"), Memory::default(), Ok(()), 200);
    assert_eq!(titles(&chat), (vec!["Друге питання".to_owned(), "Перше питання".to_owned()], 0), "newest first");
    // The other opened by its row; a failed turn's error does not follow there.
    chat.error = Some(ChatError::UsageLimit);
    assert!(chat.choose_conversation(1));
    assert_eq!((chat.current.id.clone(), chat.error.is_none(), titles(&chat).1), (first.clone(), true, 1));
    assert!(!chat.choose_conversation(5) && !chat.choose_conversation(-1));
    // A new one begun: the saved ones are a row further down, and opened by that row.
    assert!(chat.new_conversation());
    assert!(chat.choose_conversation(2));
    assert_eq!(chat.current.id, first);
    // Deleted: the newest left opens; the last deleted, a new one.
    assert!(chat.delete_conversation());
    assert_eq!((titles(&chat), chat.current.title.as_str()), ((vec!["Друге питання".to_owned()], 0), "Друге питання"));
    assert!(chat.delete_conversation());
    assert_eq!((titles(&chat), chat.current.items.len()), ((vec!["Нова".to_owned()], 0), 0));
    // No vault open: nothing to delete, nothing saved, nothing broken.
    let mut closed = state("big", None);
    assert!(!closed.delete_conversation());
    ask(&mut closed, "Питання");
    assert_eq!(closed.turn_done(answered("Питання"), Memory::default(), Ok(()), 1), None);
    assert_eq!(closed.entries.len(), 0);
}
