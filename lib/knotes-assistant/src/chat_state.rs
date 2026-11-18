//! What the chat's panel shows and asks (ADR 0027), with no panel: the model and effort
//! chosen, the conversations and the one open, the document offered, the turn under way. The
//! panel keeps one, tells it what the user did and draws what it holds.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use knotes_chat::{ChatError, ChatEvent, Conversation, ConversationEntry, Conversations, Memory, Model, Registration};
use serde_json::Value;

use crate::chat_worker::{Turn, TurnDone};

/// A turn under way: the question, the reply so far and the tools it called.
pub struct Asking {
    pub question: String,
    pub reply: String,
    pub tools: Vec<(String, Value)>,
    /// The conversation's older part is being summarized first.
    pub compacting: bool,
    pub cancel: Arc<AtomicBool>,
}

pub struct ChatState {
    pub registration: Registration,
    pub conversations: Option<Conversations>,
    /// The conversations as the list shows them, newest first.
    pub entries: Vec<ConversationEntry>,
    pub current: Conversation,
    pub document: Option<String>,
    /// The document is offered to the model (the chip under the field not crossed out).
    pub document_on: bool,
    /// The window's space (ADR 0030): its id and name, told to the model each turn.
    pub space: Option<(String, String)>,
    pub models: Vec<Model>,
    pub model: String,
    /// How hard the model is asked to reason; none, its own default.
    pub effort: Option<String>,
    pub turn: Option<Asking>,
    /// What went wrong in the last turn.
    pub error: Option<ChatError>,
    /// Set to call off the sign-in under way.
    pub sign_in: Option<Arc<AtomicBool>>,
    pub sign_in_words: String,
}

/// The effort to ask `model` for: the one `chosen` if the model takes it (or while the list of
/// models is not in yet), else none, so the model reasons as it does by default.
pub fn effort_for(models: &[Model], model: &str, chosen: Option<&String>) -> Option<String> {
    match models.iter().find(|m| m.slug == model) {
        Some(m) => chosen.filter(|e| m.efforts.contains(e)).cloned(),
        None => chosen.cloned(),
    }
}

/// Where the conversation is summarized for `model` (its window from the plan's list).
pub fn compact_at_for(models: &[Model], model: &str) -> u64 {
    knotes_chat::compact_at(models.iter().find(|m| m.slug == model).and_then(|m| m.context_window))
}

impl ChatState {
    /// As the app starts: a new conversation with the model and effort of the last launch,
    /// the models not listed yet, no vault open.
    pub fn new(registration: Registration, model: String, effort: Option<String>) -> Self {
        Self {
            registration,
            conversations: None,
            entries: Vec::new(),
            current: Conversation::new(&model),
            document: None,
            document_on: true,
            space: None,
            models: Vec::new(),
            model,
            effort,
            turn: None,
            error: None,
            sign_in: None,
            sign_in_words: String::new(),
        }
    }

    fn chosen_model(&self) -> Option<&Model> {
        self.models.iter().find(|m| m.slug == self.model)
    }

    /// The plan's models are in: the one chosen stays if it is among them, else the first; an
    /// effort the model does not take gives way to its own.
    pub fn set_models(&mut self, models: Vec<Model>) {
        if !models.iter().any(|m| m.slug == self.model) {
            self.model = models.first().map(|m| m.slug.clone()).unwrap_or_default();
        }
        self.models = models;
        self.effort = self.asked_effort();
    }

    /// The model at `index` of the list chosen: its slug, to keep for the next launch. An
    /// effort the new model does not take gives way to its own.
    pub fn choose_model(&mut self, index: isize) -> Option<String> {
        let slug = usize::try_from(index).ok().and_then(|i| self.models.get(i)).map(|m| m.slug.clone())?;
        self.model = slug.clone();
        self.effort = self.asked_effort();
        Some(slug)
    }

    /// The efforts the model chosen takes, and which is the one it will be asked for (its own
    /// default when none was chosen). None while the model is not in the list.
    pub fn efforts(&self) -> Option<(&[String], Option<usize>)> {
        let model = self.chosen_model()?;
        let chosen = self.effort.as_ref().unwrap_or(&model.default_effort);
        Some((&model.efforts, model.efforts.iter().position(|e| e == chosen)))
    }

    /// The effort at `index` of the model's chosen: asked for from now on.
    pub fn choose_effort(&mut self, index: isize) -> Option<String> {
        let effort = usize::try_from(index).ok().and_then(|i| self.chosen_model()?.efforts.get(i)).cloned()?;
        self.effort = Some(effort.clone());
        Some(effort)
    }

    /// The effort the model is asked for (`effort_for`).
    pub fn asked_effort(&self) -> Option<String> {
        effort_for(&self.models, &self.model, self.effort.as_ref())
    }

    /// The effort the model reasons with, by its name in the list: the one asked for, or the
    /// model's own default; none while neither is known.
    pub fn used_effort(&self) -> Option<String> {
        self.asked_effort().or_else(|| self.chosen_model().map(|m| m.default_effort.clone()))
    }

    /// Where the conversation is summarized for the model chosen.
    pub fn compact_at(&self) -> u64 {
        compact_at_for(&self.models, &self.model)
    }

    /// The model chosen as the user sees it; its slug while the list is not in.
    pub fn model_name(&self) -> String {
        self.chosen_model().map_or_else(|| self.model.clone(), |m| m.name.clone())
    }

    /// The list of conversations: a new one not saved yet first (`new`), then the saved, one
    /// with no title as `untitled`; and which is the one open.
    pub fn conversation_titles(&self, new: &str, untitled: &str) -> (Vec<String>, usize) {
        let saved = self.entries.iter().position(|e| e.id == self.current.id);
        let titles = saved.is_none().then(|| new.to_owned()).into_iter().chain(self.entries.iter().map(|e| if e.title.is_empty() { untitled.to_owned() } else { e.title.clone() })).collect();
        (titles, saved.unwrap_or(0))
    }

    /// The conversation at `index` of that list opened; false when there is none to open (the
    /// new one's own row, a file gone).
    pub fn choose_conversation(&mut self, index: isize) -> bool {
        let offset = usize::from(!self.entries.iter().any(|e| e.id == self.current.id));
        let Some(entry) = usize::try_from(index).ok().and_then(|i| i.checked_sub(offset)).and_then(|i| self.entries.get(i)) else { return false };
        let Some(conversation) = self.conversations.as_ref().and_then(|c| c.load(&entry.id)) else { return false };
        self.current = conversation;
        self.error = None;
        true
    }

    /// A new conversation begun; not while a turn runs, nor when the one open is new still.
    pub fn new_conversation(&mut self) -> bool {
        if self.turn.is_some() || self.current.items.is_empty() {
            return false;
        }
        self.current = Conversation::new(&self.model);
        self.error = None;
        true
    }

    /// The conversation open deleted, the newest left opened (a new one when none is); not
    /// while a turn runs.
    pub fn delete_conversation(&mut self) -> bool {
        if self.turn.is_some() {
            return false;
        }
        let Some(conversations) = self.conversations.as_ref() else { return false };
        conversations.delete(&self.current.id);
        self.entries = conversations.list();
        self.current = self.entries.first().and_then(|e| conversations.load(&e.id)).unwrap_or_else(|| Conversation::new(&self.model));
        self.error = None;
        true
    }

    /// How full the conversation is: its tokens when last sent, and where it is summarized.
    pub fn context(&self) -> Option<(u64, u64)> {
        Some((self.current.memory.tokens?, self.compact_at()))
    }

    /// The document open (a vault path), offered to the model unless crossed out; false when
    /// it is the one known already.
    pub fn set_document(&mut self, document: Option<String>) -> bool {
        if self.document == document {
            return false;
        }
        self.document = document;
        self.document_on = true;
        true
    }

    /// The turn under way told to stop; false when none runs.
    pub fn stop_turn(&self) -> bool {
        self.turn.as_ref().map(|turn| turn.cancel.store(true, Ordering::Relaxed)).is_some()
    }

    /// A turn begun with the question `text`: what the worker runs. `on_event` and `done` are
    /// the worker's words back.
    pub fn begin_turn(&mut self, text: String, on_event: Box<dyn FnMut(ChatEvent) + Send>, done: TurnDone) -> Turn {
        if self.model.is_empty() {
            self.model = self.current.model.clone();
        }
        self.current.model = self.model.clone();
        self.error = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.turn = Some(Asking { question: text.clone(), reply: String::new(), tools: Vec::new(), compacting: false, cancel: Arc::clone(&cancel) });
        Turn {
            items: self.current.items.clone(),
            memory: self.current.memory.clone(),
            compact_at: self.compact_at(),
            text,
            open: self.document.clone().filter(|_| self.document_on),
            space: self.space.clone(),
            model: self.model.clone(),
            effort: self.asked_effort(),
            cache_key: self.current.id.clone(),
            cancel,
            on_event,
            done,
        }
    }

    /// What the turn under way said; false when none runs (its last words after it was done).
    pub fn chat_event(&mut self, event: ChatEvent) -> bool {
        let Some(turn) = self.turn.as_mut() else { return false };
        match event {
            ChatEvent::Text(text) => turn.reply.push_str(&text),
            ChatEvent::ToolCall { name, arguments } => turn.tools.push((name, arguments)),
            ChatEvent::ToolDone { .. } => {}
            ChatEvent::Compacting => turn.compacting = true,
        }
        true
    }

    /// The turn is done at `now` (seconds since 1970). Done well: the conversation takes its
    /// items, is named by its first question and saved. Failed: the error is kept to show, and
    /// the question is given back, to go into the field again.
    pub fn turn_done(&mut self, items: Vec<Value>, memory: Memory, result: Result<(), ChatError>, now: u64) -> Option<String> {
        let turn = self.turn.take();
        let save = |state: &Self| {
            if let Some(conversations) = &state.conversations
                && let Err(e) = conversations.save(&state.current)
            {
                eprintln!("knotes: cannot save the conversation: {e}");
            }
        };
        // A summary made for a turn that then failed still covers items from before it.
        if memory != self.current.memory {
            self.current.memory = memory;
            if result.is_err() {
                save(self);
            }
        }
        match result {
            Ok(()) => {
                self.current.items = items;
                self.current.name_from_first_question();
                self.current.updated = now;
                save(self);
                if let Some(conversations) = &self.conversations {
                    self.entries = conversations.list();
                }
                None
            }
            Err(error) => {
                self.error = Some(error);
                turn.map(|t| t.question)
            }
        }
    }
}
