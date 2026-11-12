use knotes_core::{CaseMode, ContentScan, ScanHit, ScanQuery, TextCorpus};
use knotes_search::RegexScanner;

struct Corpus(Vec<(&'static str, &'static str)>);

impl TextCorpus for Corpus {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn note(&self, i: usize) -> (&str, &[u8]) {
        (self.0[i].0, self.0[i].1.as_bytes())
    }
}

fn scan(corpus: &Corpus, pattern: &str, regex: bool, case: CaseMode) -> Vec<ScanHit> {
    RegexScanner.scan(corpus, &ScanQuery { pattern, regex, case }, 100).unwrap()
}

fn literal(corpus: &Corpus, pattern: &str) -> Vec<(String, u32)> {
    scan(corpus, pattern, false, CaseMode::Smart).into_iter().map(|h| (h.path, h.line)).collect()
}

#[test]
fn smart_case() {
    let c = Corpus(vec![("a.md", "Radar\nradar\nRADAR\n")]);
    assert_eq!(literal(&c, "radar").len(), 3);
    assert_eq!(literal(&c, "Radar"), [("a.md".to_owned(), 1)]);
    assert_eq!(scan(&c, "radar", false, CaseMode::Sensitive).len(), 1);
}

#[test]
fn literal_mode_escapes_regex_characters() {
    let c = Corpus(vec![("a.md", "axb\na.b\n")]);
    assert_eq!(literal(&c, "a.b"), [("a.md".to_owned(), 2)]);
}

#[test]
fn regex_mode() {
    let c = Corpus(vec![("a.md", "release v1.2 and v10.0\nnot v.x\n")]);
    let hits = scan(&c, r"\bv\d+\.\d+", true, CaseMode::Smart);
    assert_eq!(hits.len(), 2);
    let text = "release v1.2 and v10.0\nnot v.x\n";
    assert_eq!(&text[hits[1].span.start as usize..hits[1].span.end as usize], "v10.0");
}

#[test]
fn lines_spans_and_line_text_in_ukrainian() {
    let text = "перший рядок\nдругий радар тут\nтретій\n";
    let c = Corpus(vec![("uk.md", text)]);
    let hits = scan(&c, "радар", false, CaseMode::Smart);
    assert_eq!(hits.len(), 1);
    let h = &hits[0];
    assert_eq!((h.line, h.line_text.as_str()), (2, "другий радар тут"));
    assert_eq!(&text[h.span.start as usize..h.span.end as usize], "радар");
}

#[test]
fn several_matches_on_one_line_and_corpus_order() {
    let c = Corpus(vec![("b.md", "x x\n"), ("a.md", "x\n")]);
    let hits: Vec<(String, u32, u32)> =
        scan(&c, "x", false, CaseMode::Smart).into_iter().map(|h| (h.path, h.line, h.span.start)).collect();
    assert_eq!(hits, [("b.md".to_owned(), 1, 0), ("b.md".to_owned(), 1, 2), ("a.md".to_owned(), 1, 0)]);
}

#[test]
fn limit_keeps_the_first_hits() {
    let c = Corpus(vec![("a.md", "x\nx\nx\n"), ("b.md", "x\n")]);
    let hits = RegexScanner.scan(&c, &ScanQuery { pattern: "x", regex: false, case: CaseMode::Smart }, 2).unwrap();
    assert_eq!(hits.iter().map(|h| h.line).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn invalid_regex_is_an_error_and_empty_pattern_finds_nothing() {
    let c = Corpus(vec![("a.md", "text\n")]);
    assert!(RegexScanner.scan(&c, &ScanQuery { pattern: "(", regex: true, case: CaseMode::Smart }, 10).is_err());
    assert!(scan(&c, "", false, CaseMode::Smart).is_empty());
}
