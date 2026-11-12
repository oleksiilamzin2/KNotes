use knotes_core::{BoxError, CaseMode, ContentScan, ScanHit, ScanQuery, Span, TextCorpus};
use rayon::prelude::*;
use regex::bytes::{Regex, RegexBuilder};

/// `line_text` keeps at most this many bytes on each side of the match: notes written as one
/// long paragraph line would otherwise copy the whole paragraph into every hit.
const CONTEXT_BYTES: usize = 160;

/// Parallel regex scan over raw note bytes (`regex::bytes`: no UTF-8 validation per scan;
/// literal patterns use the crate's SIMD prefilters).
pub struct RegexScanner;

impl ContentScan for RegexScanner {
    fn scan(&self, corpus: &impl TextCorpus, query: &ScanQuery, limit: usize) -> Result<Vec<ScanHit>, BoxError> {
        if query.pattern.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let source = if query.regex { query.pattern.to_owned() } else { regex::escape(query.pattern) };
        let insensitive = match query.case {
            CaseMode::Smart => !query.pattern.chars().any(char::is_uppercase),
            CaseMode::Sensitive => false,
            CaseMode::Insensitive => true,
        };
        let re = RegexBuilder::new(&source).case_insensitive(insensitive).build()?;
        // Each note is capped at `limit` hits; collecting keeps corpus order.
        let per_note: Vec<Vec<ScanHit>> = (0..corpus.len()).into_par_iter().map(|i| scan_note(&re, corpus, i, limit)).collect();
        Ok(per_note.into_iter().flatten().take(limit).collect())
    }
}

fn scan_note(re: &Regex, corpus: &impl TextCorpus, i: usize, limit: usize) -> Vec<ScanHit> {
    let (path, text) = corpus.note(i);
    let mut hits = Vec::new();
    let (mut line, mut counted) = (1u32, 0usize);
    for m in re.find_iter(text).filter(|m| !m.is_empty()) {
        line += text[counted..m.start()].iter().filter(|&&b| b == b'\n').count() as u32;
        counted = m.start();
        let line_start = text[..m.start()].iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1);
        let line_end = text[m.end()..].iter().position(|&b| b == b'\n').map_or(text.len(), |p| m.end() + p);
        let from = char_start(text, line_start.max(m.start().saturating_sub(CONTEXT_BYTES)));
        let to = char_end(text, line_end.min(m.end() + CONTEXT_BYTES));
        hits.push(ScanHit {
            path: path.to_owned(),
            line,
            span: Span::new(m.start(), m.end()),
            line_text: String::from_utf8_lossy(&text[from..to]).into_owned(),
        });
        if hits.len() >= limit {
            break;
        }
    }
    hits
}

/// Moves `i` forward to the start of a UTF-8 character.
fn char_start(text: &[u8], mut i: usize) -> usize {
    while i < text.len() && text[i] & 0xC0 == 0x80 {
        i += 1;
    }
    i
}

/// Moves `i` back so that `text[..i]` ends on a whole UTF-8 character.
fn char_end(text: &[u8], mut i: usize) -> usize {
    while i > 0 && i < text.len() && text[i] & 0xC0 == 0x80 {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_window_cuts_long_lines_on_char_boundaries() {
        struct One(String);
        impl TextCorpus for One {
            fn len(&self) -> usize {
                1
            }
            fn note(&self, _: usize) -> (&str, &[u8]) {
                ("a.md", self.0.as_bytes())
            }
        }
        let text = format!("{}радар{}", "ж".repeat(500), "ш".repeat(500));
        let q = ScanQuery { pattern: "радар", regex: false, case: CaseMode::Smart };
        let hits = RegexScanner.scan(&One(text), &q, 10).unwrap();
        let t = &hits[0].line_text;
        assert!(t.len() <= 2 * CONTEXT_BYTES + "радар".len(), "{}", t.len());
        assert!(t.contains("радар") && !t.contains('\u{FFFD}'));
    }
}
