//! What the user sets for a deck (ADR 0037): the chance of recall its cards come back at, and
//! how many new cards a day it asks. A deck not set takes what the nearest deck above it has.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

/// The chance of recall cards are due at, when no deck says.
pub const RETENTION: f64 = 0.9;
/// Below it reviews are few and much is forgotten; above it they come too often.
pub const RETENTION_RANGE: (f64, f64) = (0.7, 0.97);
/// New cards a day, when no deck says (Anki's).
pub const NEW_A_DAY: u32 = 20;

/// A deck's own settings: none for what it takes from above.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeckSetting {
    pub retention: Option<f64>,
    pub new_a_day: Option<u32>,
}

/// Every deck's settings, as `.knotes/decks.json` holds them:
/// `{"мови": {"retention": 0.85, "new_a_day": 10}}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeckSettings {
    decks: BTreeMap<String, DeckSetting>,
    /// What the file holds besides (a newer Knotes' keys), kept when written again.
    other: BTreeMap<String, Map<String, Value>>,
}

impl DeckSettings {
    /// What `text` sets; nothing for a text that is not the file's (empty, damaged). A retention
    /// out of range is brought into it.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        let Ok(Value::Object(decks)) = serde_json::from_str::<Value>(text) else { return settings };
        for (deck, value) in decks {
            let Value::Object(mut value) = value else { continue };
            let retention = value.remove("retention").and_then(|v| v.as_f64()).map(|r| r.clamp(RETENTION_RANGE.0, RETENTION_RANGE.1));
            let new_a_day = value.remove("new_a_day").and_then(|v| v.as_u64()).map(|n| n.min(u64::from(u32::MAX)) as u32);
            settings.decks.insert(deck.clone(), DeckSetting { retention, new_a_day });
            settings.other.insert(deck, value);
        }
        settings
    }

    /// The file's text: the decks that set something, sorted.
    pub fn to_text(&self) -> String {
        let mut decks = Map::new();
        for (deck, setting) in &self.decks {
            let mut value = self.other.get(deck).cloned().unwrap_or_default();
            if let Some(retention) = setting.retention {
                value.insert("retention".into(), json!(retention));
            }
            if let Some(new) = setting.new_a_day {
                value.insert("new_a_day".into(), json!(new));
            }
            if !value.is_empty() {
                decks.insert(deck.clone(), Value::Object(value));
            }
        }
        format!("{}\n", serde_json::to_string_pretty(&Value::Object(decks)).unwrap_or_default())
    }

    /// What `deck` itself sets.
    pub fn own(&self, deck: &str) -> DeckSetting {
        self.decks.get(deck).copied().unwrap_or_default()
    }

    /// Sets `deck`'s own.
    pub fn set(&mut self, deck: &str, setting: DeckSetting) {
        self.decks.insert(deck.to_owned(), setting);
    }

    /// The chance of recall `deck`'s cards come back at: its own, or the nearest deck's above.
    pub fn retention(&self, deck: &str) -> f64 {
        self.nearest(deck, |s| s.retention).unwrap_or(RETENTION)
    }

    /// How many new cards a day `deck` asks (its own with those of the decks under it).
    pub fn new_a_day(&self, deck: &str) -> u32 {
        self.nearest(deck, |s| s.new_a_day).unwrap_or(NEW_A_DAY)
    }

    /// What the deck above `deck` gives it: what `deck` has when it sets nothing itself.
    pub fn inherited(&self, deck: &str) -> (f64, u32) {
        match deck.rsplit_once('/') {
            Some((above, _)) => (self.retention(above), self.new_a_day(above)),
            None => (RETENTION, NEW_A_DAY),
        }
    }

    fn nearest<T>(&self, deck: &str, of: impl Fn(&DeckSetting) -> Option<T>) -> Option<T> {
        let mut at = Some(deck);
        while let Some(deck) = at {
            if let Some(found) = self.decks.get(deck).and_then(&of) {
                return Some(found);
            }
            at = deck.rsplit_once('/').map(|(above, _)| above);
        }
        None
    }
}
