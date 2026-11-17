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
/// The minutes of a new card's steps, when no deck says (the reference's, and Anki's).
pub const STEPS: [u32; 2] = [1, 10];
/// A step is a day at most: longer is the schedule's to give.
const STEP_MAX: u64 = 24 * 60;

/// A deck's own settings: none for what it takes from above.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeckSetting {
    pub retention: Option<f64>,
    pub new_a_day: Option<u32>,
    /// The minutes of the steps a new card is learned in (empty: none, its days begin at once).
    pub steps: Option<Vec<u32>>,
}

/// Every deck's settings, as `.knotes/decks.json` holds them:
/// `{"мови": {"retention": 0.85, "new_a_day": 10, "steps": [1, 10]}}`. The deck `""` is above
/// every deck: what it sets, all take.
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
            let steps = value.remove("steps").and_then(|v| v.as_array().map(|steps| steps.iter().filter_map(|m| m.as_u64()).filter(|m| *m > 0).map(|m| m.min(STEP_MAX) as u32).collect()));
            settings.decks.insert(deck.clone(), DeckSetting { retention, new_a_day, steps });
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
            if let Some(steps) = &setting.steps {
                value.insert("steps".into(), json!(steps));
            }
            if !value.is_empty() {
                decks.insert(deck.clone(), Value::Object(value));
            }
        }
        format!("{}\n", serde_json::to_string_pretty(&Value::Object(decks)).unwrap_or_default())
    }

    /// What `deck` itself sets.
    pub fn own(&self, deck: &str) -> DeckSetting {
        self.decks.get(deck).cloned().unwrap_or_default()
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

    /// The minutes of the steps a new card of `deck` is learned in.
    pub fn step_minutes(&self, deck: &str) -> Vec<u32> {
        self.nearest(deck, |s| s.steps.clone()).unwrap_or_else(|| STEPS.to_vec())
    }

    /// The steps `deck`'s cards are learned in.
    pub fn steps(&self, deck: &str) -> crate::Steps {
        crate::Steps::of_minutes(&self.step_minutes(deck))
    }

    /// What the deck above `deck` gives it: what `deck` has when it sets nothing itself (the
    /// chance of recall, the new cards a day, the steps' minutes).
    pub fn inherited(&self, deck: &str) -> (f64, u32, Vec<u32>) {
        let above = deck.rsplit_once('/').map_or("", |(above, _)| above);
        (self.retention(above), self.new_a_day(above), self.step_minutes(above))
    }

    fn nearest<T>(&self, deck: &str, of: impl Fn(&DeckSetting) -> Option<T>) -> Option<T> {
        let mut at = Some(deck);
        while let Some(deck) = at {
            if let Some(found) = self.decks.get(deck).and_then(&of) {
                return Some(found);
            }
            // Above a top deck: the vault's own, `""`.
            at = deck.rsplit_once('/').map(|(above, _)| above).or((!deck.is_empty()).then_some(""));
        }
        None
    }
}
