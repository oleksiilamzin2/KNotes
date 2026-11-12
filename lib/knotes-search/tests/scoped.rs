use knotes_core::{CaseMode, ContentScan, ScanQuery, TextCorpus};
use knotes_search::{RegexScanner, ScopedCorpus};

struct Corpus(Vec<(&'static str, &'static str)>);

impl TextCorpus for Corpus {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn note(&self, i: usize) -> (&str, &[u8]) {
        (self.0[i].0, self.0[i].1.as_bytes())
    }
}

fn scoped_paths(corpus: &Corpus, scope: &[&str]) -> Vec<String> {
    let scope: Vec<String> = scope.iter().map(|s| s.to_string()).collect();
    let scoped = ScopedCorpus::new(corpus, &scope);
    let q = ScanQuery { pattern: "radar", regex: false, case: CaseMode::Smart };
    RegexScanner.scan(&scoped, &q, 100).unwrap().into_iter().map(|h| h.path).collect()
}

#[test]
fn scan_sees_only_scoped_notes_in_corpus_order() {
    let c = Corpus(vec![("a.md", "radar"), ("b.md", "radar"), ("c.md", "radar")]);
    assert_eq!(scoped_paths(&c, &["c.md", "a.md", "missing.md"]), ["a.md", "c.md"]);
}

#[test]
fn empty_scope_is_empty() {
    let c = Corpus(vec![("a.md", "radar")]);
    assert!(scoped_paths(&c, &[]).is_empty());
    assert!(ScopedCorpus::new(&c, &[]).is_empty());
}
