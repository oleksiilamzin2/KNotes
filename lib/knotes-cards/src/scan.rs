//! The cards a note's text holds. A card is a paragraph that starts with its deck, `#@deck/sub`
//! (after a list marker, if it is a list item's), and reads one of three ways:
//!
//! - `#@deck question::answer` — on a line;
//! - `#@deck question`, a line `?`, the answer — over several lines;
//! - `#@deck a text with ==words== left out` — a cloze.
//!
//! Its id, `^c-a1b2c3`, ends its last line (as Obsidian's block ids do). Code blocks and the
//! frontmatter hold no cards.

use std::ops::Range;

use crate::{Card, CardKind};

/// What starts a card, before its deck's path.
pub const DECK_MARK: &str = "#@";
/// What starts a card's id, at its end.
pub const ID_MARK: &str = "^c-";

/// The cards of `text`, in order.
pub fn scan_cards(text: &str) -> Vec<Card> {
    let mut cards = Vec::new();
    // The lines of the card being read: each one's range in `text`.
    let mut open: Vec<Range<usize>> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut at = 0;
    let frontmatter_end = frontmatter_end(text);
    for line in text.split_inclusive('\n') {
        let range = at..at + line.trim_end_matches(['\n', '\r']).len();
        at += line.len();
        let content = &text[range.clone()];
        if range.start < frontmatter_end {
            continue;
        }
        // A fenced code block, from its fence to one as long of the same character.
        let trimmed = content.trim_start();
        let run = |c: char| trimmed.chars().take_while(|&x| x == c).count();
        match fence {
            Some((c, len)) => {
                if run(c) >= len && trimmed.trim_start_matches(c).trim().is_empty() {
                    fence = None;
                }
                continue;
            }
            None => {
                if let Some(c) = ['`', '~'].into_iter().find(|&c| run(c) >= 3) {
                    close(text, &mut open, &mut cards);
                    fence = Some((c, run(c)));
                    continue;
                }
            }
        }
        if content.trim().is_empty() {
            close(text, &mut open, &mut cards);
        } else if deck_of(after_marker(content)).is_some() {
            // A card right under another, no blank line between.
            close(text, &mut open, &mut cards);
            open.push(range);
        } else if !open.is_empty() {
            open.push(range);
        }
    }
    close(text, &mut open, &mut cards);
    cards
}

/// Where the frontmatter ends (0 without one): the `---` lines the text starts with.
fn frontmatter_end(text: &str) -> usize {
    if !text.starts_with("---\n") && !text.starts_with("---\r\n") {
        return 0;
    }
    let mut at = 0;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        at += line.len();
        if i > 0 && line.trim_end() == "---" {
            return at;
        }
    }
    0
}

/// `line` after a list item's marker (`- `, `* `, `+ `, `1. `) and its indent.
fn after_marker(line: &str) -> &str {
    let line = line.trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest.trim_start();
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    match line[digits..].strip_prefix(". ").or_else(|| line[digits..].strip_prefix(") ")) {
        Some(rest) if digits > 0 => rest.trim_start(),
        _ => line,
    }
}

/// The deck `text` starts with and what follows it: `#@a/b rest` gives `a/b` and `rest`.
fn deck_of(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix(DECK_MARK)?;
    let len = rest.char_indices().find(|&(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))).map_or(rest.len(), |(i, _)| i);
    let deck = rest[..len].trim_matches('/');
    let after = &rest[len..];
    (!deck.is_empty() && (after.is_empty() || after.starts_with(char::is_whitespace))).then(|| (deck, after.trim_start()))
}

/// The card `lines` make, if they make one, onto `cards`; the lines are let go.
fn close(text: &str, lines: &mut Vec<Range<usize>>, cards: &mut Vec<Card>) {
    let lines = std::mem::take(lines);
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else { return };
    let Some((deck, start)) = deck_of(after_marker(&text[first.clone()])) else { return };
    // The id ends the last line.
    let last_text = &text[last.clone()];
    let id = last_text.rfind(ID_MARK).filter(|&i| {
        let id = &last_text[i + 1..];
        (i == 0 || last_text[..i].ends_with(char::is_whitespace)) && id.len() > 2 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    });
    let without_id = |line: &Range<usize>, content: &str| -> String {
        match id {
            Some(i) if line == last => content[..content.len() - (last_text.len() - i)].trim_end().to_owned(),
            _ => content.to_owned(),
        }
    };
    // Its lines, the first without the deck, the last without the id.
    let mut body: Vec<String> = Vec::with_capacity(lines.len());
    for (n, line) in lines.iter().enumerate() {
        let content = if n == 0 { start } else { &text[line.clone()] };
        body.push(without_id(line, content));
    }
    while body.last().is_some_and(|l| l.trim().is_empty()) {
        body.pop();
    }
    let made = if let Some(split) = body.iter().position(|l| l.trim() == "?") {
        let (front, back) = (body[..split].join("\n"), body[split + 1..].join("\n"));
        (!front.trim().is_empty() && !back.trim().is_empty()).then(|| (CardKind::Basic, front, back))
    } else if let Some((front, back)) = body.first().and_then(|l| l.split_once("::")) {
        let back = std::iter::once(back.to_owned()).chain(body[1..].iter().cloned()).collect::<Vec<_>>().join("\n");
        (!front.trim().is_empty() && !back.trim().is_empty()).then(|| (CardKind::Basic, front.to_owned(), back))
    } else {
        cloze(&body.join("\n")).map(|(front, back)| (CardKind::Cloze, front, back))
    };
    if let Some((kind, front, back)) = made {
        cards.push(Card {
            deck: deck.to_owned(),
            id: id.map(|i| last_text[i + 1..].to_owned()),
            kind,
            front: front.trim().to_owned(),
            back: back.trim().to_owned(),
            span: first.start..last.end,
        });
    }
}

/// A text with `==words==` left out: it with `[…]` in their place, and it whole; none when it
/// leaves nothing out.
fn cloze(text: &str) -> Option<(String, String)> {
    let (mut front, mut back, mut rest, mut found) = (String::new(), String::new(), text, false);
    while let Some(open) = rest.find("==") {
        let after = &rest[open + 2..];
        let Some(len) = after.find("==").filter(|&len| len > 0 && !after[..len].contains('\n')) else { break };
        front.push_str(&rest[..open]);
        front.push_str("[…]");
        back.push_str(&rest[..open]);
        back.push_str(&after[..len]);
        rest = &after[len + 2..];
        found = true;
    }
    front.push_str(rest);
    back.push_str(rest);
    found.then_some((front, back))
}
