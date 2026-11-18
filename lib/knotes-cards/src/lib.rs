//! Spaced-repetition cards (ADR 0037): written in a note's text, each in a deck (`#@deck/sub`
//! at its start), and scheduled by FSRS. No files, no clock: text in, cards out; a memory and
//! a grade in, the next memory and interval out.

mod card;
mod deck_settings;
mod fsrs;
mod make;
mod runs;
mod scan;
mod schedule;

pub use card::{Card, CardKind};
pub use deck_settings::{DeckSetting, DeckSettings, NEW_A_DAY, RETENTION, RETENTION_RANGE, STEPS};
pub use fsrs::{Fsrs, Grade, Memory};
pub use make::{asks, card_id, give_id, make_card};
pub use runs::{CardLink, Run, text_runs};
pub use scan::{CardParts, DECK_MARK, ID_MARK, card_parts, scan_cards};
pub use schedule::{Logged, Schedule, Steps, parse_logged, parse_review, paused_line, review, review_line, schedule, taken_back_line};
