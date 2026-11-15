use std::sync::LazyLock;

use knotes_core::BlockHit;
use knotes_embed::Passage;
use regex::{Captures, Regex};

/// What `block` of the document at `path` (its text `text`) is embedded as: its text with the
/// document's title, its section's heading before it unless the text starts with it, and its
/// dates in numbers told in words as well (a query asks for "October", a note says 2026-10-01).
pub fn block_passage(path: &str, text: &str, block: &BlockHit) -> Passage {
    let body = text.get(block.span.start as usize..block.span.end as usize).unwrap_or_default().trim();
    let body = &dates_in_words(body);
    let text = match &block.heading {
        Some(heading) if !body.lines().next().is_some_and(|first| first.contains(heading.as_str())) => format!("{heading} › {body}"),
        _ => body.to_string(),
    };
    Passage::Document { title: title_of(path), text }
}

/// Ukrainian months, as a date says them ("1 жовтня") and as a month alone ("жовтень").
const OF_MONTH: [&str; 12] = ["січня", "лютого", "березня", "квітня", "травня", "червня", "липня", "серпня", "вересня", "жовтня", "листопада", "грудня"];
const MONTH: [&str; 12] = ["січень", "лютий", "березень", "квітень", "травень", "червень", "липень", "серпень", "вересень", "жовтень", "листопад", "грудень"];

/// 2026-10-01, 01.10.2026 and 10.2026, not inside longer numbers.
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:(\d{4})-(\d{2})-(\d{2})|(\d{1,2})\.(\d{1,2})\.(\d{4})|(\d{1,2})\.(\d{4}))\b").expect("a valid pattern"));

/// `text` with each date in numbers followed by the same in words: "2026-10-01 (1 жовтня 2026)".
fn dates_in_words(text: &str) -> String {
    DATE.replace_all(text, |c: &Captures| {
        let number = |i: usize| c.get(i).and_then(|m| m.as_str().parse::<usize>().ok());
        let words = match (number(1), number(2), number(3), number(4), number(5), number(6), number(7), number(8)) {
            (Some(y), Some(m), Some(d), ..) | (_, _, _, Some(d), Some(m), Some(y), ..) if (1..=31).contains(&d) && (1..=12).contains(&m) => Some(format!("{d} {} {y}", OF_MONTH[m - 1])),
            (.., Some(m), Some(y)) if (1..=12).contains(&m) => Some(format!("{} {y}", MONTH[m - 1])),
            _ => None,
        };
        match words {
            Some(words) => format!("{} ({words})", &c[0]),
            None => c[0].to_string(),
        }
    })
    .into_owned()
}

/// A document's title: its file name without the extension.
fn title_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
