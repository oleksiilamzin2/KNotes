//! A card written into a note's text (ADR 0037): from a selection, or an id given to one.

use std::ops::Range;

use crate::{DECK_MARK, ID_MARK, scan_cards};

/// A card's id from `seed` (a time, a counter): `c-` and six letters and digits.
pub fn card_id(seed: u64) -> String {
    // Mixed (SplitMix64), so ids made one after another share no start.
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let id: String = (0..6).map(|i| digits[((x >> (i * 10)) % 36) as usize] as char).collect();
    format!("c-{id}")
}

/// Whether `selected` reads as a card already: a question and its answer, or a cloze.
pub fn asks(selected: &str) -> bool {
    !scan_cards(&format!("{DECK_MARK}x {}", selected.trim())).is_empty()
}

/// The edit that makes `text[selection]` a card of `deck` with the id `id`: the range to
/// replace and what with. What is selected is the answer to `question`; without one it must
/// read as a card itself (`asks`). The card gets a paragraph of its own (blank lines around it,
/// none inside it). None when nothing is selected, or no card comes of it.
pub fn make_card(text: &str, selection: Range<usize>, deck: &str, question: Option<&str>, id: &str) -> Option<(Range<usize>, String)> {
    let chosen = text.get(selection.clone())?;
    let start = selection.start + (chosen.len() - chosen.trim_start().len());
    let selected = chosen.trim();
    let deck = deck.trim().trim_matches('/');
    if selected.is_empty() || deck.is_empty() {
        return None;
    }
    // No blank line inside a card: it would end it.
    let lines: Vec<&str> = selected.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let body = match question.map(str::trim).filter(|q| !q.is_empty()) {
        Some(question) if lines.len() == 1 => format!("{question}::{}", lines[0]),
        Some(question) => format!("{question}\n?\n{}", lines.join("\n")),
        None => lines.join("\n"),
    };
    let card = format!("{DECK_MARK}{deck} {body} ^{id}");
    (scan_cards(&card).len() == 1).then_some(())?;
    let range = start..start + selected.len();
    let before = match &text[..range.start] {
        b if b.is_empty() || b.ends_with("\n\n") => "",
        b if b.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    let after = match &text[range.end..] {
        a if a.is_empty() || a.starts_with("\n\n") => "",
        a if a.starts_with('\n') => "\n",
        _ => "\n\n",
    };
    Some((range, format!("{before}{card}{after}")))
}

/// The edit that gives the card at `span` in `text` (as `scan_cards` found it, with no id) the
/// id `id`: where to insert and what. None when no such card is there.
pub fn give_id(text: &str, span: Range<usize>, id: &str) -> Option<(usize, String)> {
    let card = scan_cards(text).into_iter().find(|c| c.span == span && c.id.is_none())?;
    Some((card.span.end, format!(" {}{}", ID_MARK.trim_end_matches("c-"), id)))
}
