use std::sync::LazyLock;

use knotes_core::BlockHit;
use knotes_embed::Passage;
use regex::{Captures, Regex};

/// A piece of a long block holds whole lines up to about this many characters: a list's or a
/// table's lines in one vector blur each other (a note's line on blood pressure lost to a travel
/// list). With EmbeddingGemma-300M pieces did not help (ADR 0028); with the models of ADR 0029,
/// 400 raised MRR by 0.02–0.05 on both, 320 and 600 lost answers (2026-11-15).
pub const PIECE_CHARS: usize = 400;

/// What `block` of the document at `path` (its text `text`) is embedded as: its text with the
/// document's title, its section's heading before it unless the text starts with it, and its
/// dates in numbers told in words as well (a query asks for "October", a note says 2026-10-01).
pub fn block_passage(path: &str, text: &str, block: &BlockHit) -> Passage {
    passage(path, text.get(block.span.start as usize..block.span.end as usize).unwrap_or_default(), block.heading.as_deref())
}

/// The pieces `block` is embedded in, each its own place (the block's, its span narrowed to its
/// lines) and passage: the block whole when short, else runs of whole lines of about
/// `piece_chars`, a table's rows each run with the table's header row.
pub fn block_pieces(path: &str, text: &str, block: &BlockHit, piece_chars: usize) -> Vec<(BlockHit, Passage)> {
    let (start, end) = (block.span.start as usize, block.span.end as usize);
    let whole = || vec![(block.clone(), block_passage(path, text, block))];
    let Some(body) = text.get(start..end) else { return whole() };
    if body.trim().chars().count() <= piece_chars {
        return whole();
    }
    // The block's lines with where they lie in the note, blank lines and table separators left out.
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut at = start;
    for line in body.split('\n') {
        if !line.trim().is_empty() && !is_separator(line) {
            lines.push((at, at + line.len()));
        }
        at += line.len() + 1;
    }
    let rows: Vec<&str> = body.split('\n').collect();
    let header = rows.windows(2).find(|pair| is_row(pair[0]) && is_separator(pair[1])).map(|pair| pair[0].trim());
    let line_text = |(s, e): (usize, usize)| &text[s..e];
    let mut pieces = Vec::new();
    let mut run: Vec<(usize, usize)> = Vec::new();
    let mut chars = 0;
    let mut flush = |run: &mut Vec<(usize, usize)>| {
        let (Some(first), Some(last)) = (run.first().copied(), run.last().copied()) else { return };
        let mut embedded: Vec<&str> = run.iter().map(|&l| line_text(l)).collect();
        if let Some(header) = header
            && embedded.iter().any(|l| is_row(l))
            && !embedded.iter().any(|l| l.trim() == header)
        {
            embedded.insert(0, header);
        }
        let piece = BlockHit { span: knotes_core::Span { start: first.0 as u32, end: last.1 as u32 }, ..block.clone() };
        pieces.push((piece, passage(path, &embedded.join("\n"), block.heading.as_deref())));
        run.clear();
    };
    for line in lines {
        let length = line_text(line).chars().count();
        if !run.is_empty() && chars + length > piece_chars {
            flush(&mut run);
            chars = 0;
        }
        run.push(line);
        chars += length + 1;
    }
    flush(&mut run);
    pieces
}

/// A table row: `| … |`.
fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// A table's separator row: `|---|:--|`.
fn is_separator(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|') && line.contains('-') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// `body` of the document at `path`, under `heading`, as the model takes it.
fn passage(path: &str, body: &str, heading: Option<&str>) -> Passage {
    let body = dates_in_words(body.trim());
    let text = match heading {
        Some(heading) if !body.lines().next().is_some_and(|first| first.contains(heading)) => format!("{heading} › {body}"),
        _ => body,
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
