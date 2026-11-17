//! Spaced-repetition cards (ADR 0037): written in a note's text, each in a deck (`#@deck/sub`
//! at its start), and scheduled by FSRS. No files, no clock: text in, cards out; a memory and
//! a grade in, the next memory and interval out.

mod card;
mod fsrs;
mod scan;

pub use card::{Card, CardKind};
pub use fsrs::{Fsrs, Grade, Memory};
pub use scan::{DECK_MARK, ID_MARK, scan_cards};
