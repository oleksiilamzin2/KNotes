//! The cards a note's text holds. A card is a paragraph that starts with its deck, `#@deck/sub`
//! (after a list marker, if it is a list item's), and reads one of three ways:
//!
//! - `#@deck question::answer` — on a line;
//! - `#@deck question`, a line `?`, the answer — over several lines;
//! - `#@deck a text with ==words== left out` — a cloze.
//!
//! Or in braces, where blank lines do not end it (a question or an answer of several
//! paragraphs): a line `#@deck{`, the question, a line `?`, the answer, a line `}`.
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
    // A card in braces being read: its deck, where its first line starts, its lines so far.
    let mut block: Option<(&str, usize, Vec<&str>)> = None;
    let mut at = 0;
    let frontmatter_end = frontmatter_end(text);
    for line in text.split_inclusive('\n') {
        let range = at..at + line.trim_end_matches(['\n', '\r']).len();
        at += line.len();
        let content = &text[range.clone()];
        if range.start < frontmatter_end {
            continue;
        }
        // In braces: every line is the card's, blank ones too, until the line that closes it.
        if let Some((deck, start, lines)) = &mut block {
            match closing(content) {
                Some(id) => {
                    if let Some((kind, front, back)) = asked(lines) {
                        cards.push(Card { deck: (*deck).to_owned(), id: id.map(str::to_owned), kind, front, back, span: *start..range.end });
                    }
                    block = None;
                }
                None => lines.push(content),
            }
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
        if let Some(deck) = block_deck(after_marker(content)) {
            close(text, &mut open, &mut cards);
            block = Some((deck, range.start, Vec::new()));
        } else if content.trim().is_empty() {
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

/// Where a card's parts are in its note's text, for showing it (bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardParts {
    /// The whole card, its deck's mark (or a list item's marker before it) to its id.
    pub span: Range<usize>,
    /// Its deck's mark, `#@deck/sub`.
    pub deck: Range<usize>,
    /// Its question; none for a cloze.
    pub question: Option<Range<usize>>,
    /// What parts it, in order: the brace that opens a card in braces, the `::`, the `?` of its
    /// line, the closing brace.
    pub marks: Vec<Range<usize>>,
}

/// The parts of each card of `text`, in order.
pub fn card_parts(text: &str) -> Vec<CardParts> {
    scan_cards(text).iter().filter_map(|card| parts_of(text, card)).collect()
}

fn parts_of(text: &str, card: &Card) -> Option<CardParts> {
    let body = &text[card.span.clone()];
    let mark = body.find(DECK_MARK)?;
    let path = &body[mark + DECK_MARK.len()..];
    let path_len = path.char_indices().find(|&(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))).map_or(path.len(), |(i, _)| i);
    let first_line = body.find('\n').unwrap_or(body.len());
    let deck_end = mark + DECK_MARK.len() + path_len;
    let block = body[deck_end..first_line].trim() == "{";
    let within = |range: Range<usize>| card.span.start + range.start..card.span.start + range.end;
    // A range of the body without the blanks at its ends; none when nothing is left.
    let trimmed = |range: Range<usize>| {
        let part = &body[range.clone()];
        let start = range.start + (part.len() - part.trim_start().len());
        let end = range.end - (part.len() - part.trim_end().len());
        (start < end).then_some(start..end)
    };
    let mut marks = Vec::new();
    let mut question = None;
    // The question starts after the deck, in braces after the brace.
    let mut asked_from = deck_end;
    if block {
        let brace = deck_end + body[deck_end..first_line].find('{')?;
        marks.push(within(brace..brace + 1));
        asked_from = brace + 1;
    }
    if card.kind == CardKind::Basic {
        // The `?` line (not the first: that one holds the deck), else the first line's `::`.
        let mut at = 0;
        let mut asks = None;
        for line in body.split_inclusive('\n') {
            if at > 0 && asks.is_none() && line.trim() == "?" {
                asks = Some((at, at + line.find('?')?));
            }
            at += line.len();
        }
        match asks {
            Some((line, sign)) => {
                marks.push(within(sign..sign + 1));
                question = trimmed(asked_from..line);
            }
            None => {
                let sep = asked_from + body[asked_from..first_line].find("::")?;
                marks.push(within(sep..sep + 2));
                question = trimmed(asked_from..sep);
            }
        }
    }
    if block {
        let last = body.rfind('\n').map_or(0, |i| i + 1);
        let brace = last + body[last..].find('}')?;
        marks.push(within(brace..brace + 1));
    }
    Some(CardParts { span: card.span.clone(), deck: within(mark..deck_end), question: question.map(within), marks })
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

/// The deck of a line that opens a card in braces: `#@a/b{`.
fn block_deck(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(DECK_MARK)?;
    let len = rest.char_indices().find(|&(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))).map_or(rest.len(), |(i, _)| i);
    let deck = rest[..len].trim_matches('/');
    (!deck.is_empty() && rest[len..].trim() == "{").then_some(deck)
}

/// Whether `line` closes a card in braces (`}`), and the id after it if it has one (`} ^c-1`).
fn closing(line: &str) -> Option<Option<&str>> {
    let rest = line.trim().strip_prefix('}')?.trim();
    if rest.is_empty() {
        return Some(None);
    }
    let id = rest.strip_prefix('^')?;
    (id.starts_with("c-") && id.len() > 2 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).then_some(Some(id))
}

/// What the lines inside braces ask: a question and an answer apart on a `?` line, or a cloze.
fn asked(lines: &[&str]) -> Option<(CardKind, String, String)> {
    let (kind, front, back) = match lines.iter().position(|l| l.trim() == "?") {
        Some(split) => (CardKind::Basic, lines[..split].join("\n"), lines[split + 1..].join("\n")),
        None => {
            let (front, back) = cloze(&lines.join("\n"))?;
            (CardKind::Cloze, front, back)
        }
    };
    let (front, back) = (front.trim().to_owned(), back.trim().to_owned());
    (!front.is_empty() && !back.is_empty()).then_some((kind, front, back))
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
