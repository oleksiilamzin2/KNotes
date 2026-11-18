use knotes_app::{Offered, offer_blocks, offer_headings, offer_notes, offer_tags, recent_notes, step_found, tag_tree};
use knotes_core::{LinkKind, LinkResolver, NoteParser, NoteRow};

/// Links by a note's name alone, unless two notes have it.
struct ByName(&'static [&'static str]);

impl LinkResolver for ByName {
    fn resolve(&self, _kind: LinkKind, _target: &str, _source: &str) -> Option<String> {
        None
    }

    fn link_text(&self, path: &str, _source: &str) -> String {
        let name = |p: &str| p.rsplit('/').next().unwrap_or(p).trim_end_matches(".md").to_owned();
        if self.0.iter().filter(|other| name(other) == name(path)).count() > 1 { path.trim_end_matches(".md").to_owned() } else { name(path) }
    }
}

fn offered(title: &str, detail: &str, insert: &str) -> Offered {
    Offered { title: title.into(), detail: detail.into(), insert: insert.into() }
}

#[test]
fn a_link_typed_is_offered_notes_with_the_shortest_link_to_each() {
    let vault = ByName(&["Work/Plan.md", "Home/Plan.md", "Radar.md"]);
    let found = vec![("Radar.md".to_owned(), None), ("Work/Plan.md".to_owned(), None), ("Radar.md".to_owned(), Some("Радар".to_owned()))];
    assert_eq!(
        offer_notes(found, "a.md", &vault),
        [offered("Radar", "", "Radar"), offered("Plan", "Work", "Work/Plan"), offered("Радар", "", "Radar|Радар")],
        "its name and folder; the path where two share a name; found by an alias, the link shows it"
    );
    // Nothing typed yet: the notes changed last, the latest first.
    let rows: Vec<NoteRow> = [("old.md", 1), ("new.md", 9), ("mid.md", 5), ("older.md", 0)].into_iter().map(|(path, mtime_ns)| NoteRow { path: path.into(), mtime_ns, snippet: String::new() }).collect();
    assert_eq!(recent_notes(&rows, 2), ["new.md", "mid.md"]);
    assert_eq!(recent_notes(&rows, 10).len(), 4);
    assert!(recent_notes(&rows, 0).is_empty() && recent_notes(&[], 5).is_empty());
}

#[test]
fn a_heading_a_block_or_a_tag_typed_is_offered_those_holding_it() {
    let text = "# Budget\n\nThe plan for the year. ^plan-2026\n\n## Budget cuts\n\n- one ^cut\n\n### Other\n";
    let parsed = knotes_md::MarkdownParser::new().parse(text);
    let titles = |offered: Vec<Offered>| offered.into_iter().map(|o| format!("{} ({})", o.title, o.detail)).collect::<Vec<_>>().join(", ");
    assert_eq!(titles(offer_headings(parsed.headings.clone(), "BUDGET", 10)), "Budget (H1), Budget cuts (H2)");
    assert_eq!(titles(offer_headings(parsed.headings.clone(), "", 2)), "Budget (H1), Budget cuts (H2)", "no more than asked, in the note's order");
    assert_eq!(offer_headings(parsed.headings.clone(), "other", 10)[0].insert, "Other");
    // A block by its anchor, with the start of its line to tell it by.
    assert_eq!(titles(offer_blocks(text, parsed.anchors.clone(), "", 10)), "^plan-2026 (The plan for the year.), ^cut (- one)");
    assert_eq!(offer_blocks(text, parsed.anchors.clone(), "CUT", 10), [offered("^cut", "- one", "^cut")]);
    assert!(offer_blocks(text, parsed.anchors, "zz", 10).is_empty());
    // A tag with its notes: `#` shown in the text, none in the frontmatter; the same goes in.
    let tags = tag_tree(&[("work".into(), 5), ("work/radar".into(), 2), ("home".into(), 1)]);
    assert_eq!(offer_tags("wo", false, &[], &tags, 10)[0], offered("#work", "5", "work"));
    assert_eq!(offer_tags("wo", true, &[], &tags, 10)[0], offered("work", "5", "work"));
    assert!(offer_tags("zz", false, &[], &tags, 10).is_empty());
}

#[test]
fn the_finds_are_stepped_through_round_their_ends() {
    assert_eq!((step_found(None, 3, true), step_found(None, 3, false)), (Some(0), Some(0)), "none shown: the first");
    assert_eq!((step_found(Some(0), 3, true), step_found(Some(2), 3, true)), (Some(1), Some(0)));
    assert_eq!((step_found(Some(1), 3, false), step_found(Some(0), 3, false)), (Some(0), Some(2)));
    assert_eq!((step_found(Some(0), 1, true), step_found(Some(0), 1, false)), (Some(0), Some(0)));
    assert_eq!((step_found(None, 0, true), step_found(Some(2), 0, false)), (None, None));
}
