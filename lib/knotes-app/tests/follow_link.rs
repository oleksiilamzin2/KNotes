use knotes_app::{Followed, follow_link, linked_note_path, named_link, normalize_path};
use knotes_core::{Link, LinkKind, LinkResolver, Span};

/// Resolves a target to itself with `.md` (or as written, for a PDF) when the vault has it;
/// a link with no target, to its source.
struct Vault(&'static [&'static str]);

impl LinkResolver for Vault {
    fn resolve(&self, _kind: LinkKind, target: &str, source: &str) -> Option<String> {
        if target.is_empty() {
            return Some(source.to_owned());
        }
        let path = if target.ends_with(".pdf") { target.to_owned() } else { format!("{target}.md") };
        self.0.contains(&path.as_str()).then_some(path)
    }

    fn link_text(&self, path: &str, _source: &str) -> String {
        path.to_owned()
    }
}

fn link(kind: LinkKind, target: &str, subpath: Option<&str>) -> Link {
    Link { kind, embed: false, target: target.into(), subpath: subpath.map(str::to_owned), display: None, span: Span { start: 0, end: 0 } }
}

#[test]
fn a_link_leads_to_a_note_a_pdf_the_web_or_a_note_still_to_make() {
    let vault = Vault(&["Projects/Radar.md", "budget.pdf"]);
    let links = Some(&vault);
    let follow = |l: &Link| follow_link(l, "a.md", links);
    assert_eq!(follow(&link(LinkKind::Wiki, "Projects/Radar", Some("Budget"))), Followed::Note { path: "Projects/Radar.md".into(), subpath: Some("Budget".into()) });
    assert_eq!(follow(&link(LinkKind::Wiki, "", Some("Top"))), Followed::Note { path: "a.md".into(), subpath: Some("Top".into()) }, "a heading of the note itself");
    assert_eq!(follow(&link(LinkKind::Wiki, "budget.pdf", Some("page=3"))), Followed::Pdf { path: "budget.pdf".into(), page: Some(3) });
    assert_eq!(follow(&link(LinkKind::Markdown, "budget.pdf", Some("Totals"))), Followed::Pdf { path: "budget.pdf".into(), page: None }, "a PDF has pages, not headings");
    assert_eq!(follow(&link(LinkKind::Wiki, "Nowhere", None)), Followed::Missing);
    // The web's address needs no vault; anything else waits for the first snapshot.
    let web = link(LinkKind::External, "https://example.com/a#b", None);
    assert_eq!(follow_link::<Vault>(&web, "a.md", None), Followed::Web("https://example.com/a#b".into()));
    assert_eq!(follow_link::<Vault>(&link(LinkKind::Wiki, "Projects/Radar", None), "a.md", None), Followed::Nothing);
    // As a chat's answer or a card names it.
    assert_eq!(follow(&named_link("Projects/Radar#Budget")), Followed::Note { path: "Projects/Radar.md".into(), subpath: Some("Budget".into()) });
    assert_eq!(follow(&named_link("budget.pdf#page=12")), Followed::Pdf { path: "budget.pdf".into(), page: Some(12) });
    assert_eq!(follow(&named_link("Nowhere")), Followed::Missing);
}

#[test]
fn a_note_a_link_names_is_made_where_the_link_says_or_among_the_new_notes() {
    let made = |kind, target: &str, source: &str| linked_note_path(&link(kind, target, None), source, "Вхідні");
    // A bare name: the folder of new notes. With a folder: there, from the vault's top.
    assert_eq!(made(LinkKind::Wiki, "Idea", "Work/a.md").as_deref(), Some("Вхідні/Idea.md"));
    assert_eq!(made(LinkKind::Wiki, "Projects/Idea", "Work/a.md").as_deref(), Some("Projects/Idea.md"));
    assert_eq!(made(LinkKind::Wiki, "/Idea.MD", "Work/a.md").as_deref(), Some("Вхідні/Idea.MD"), "its own `.md`, in any case, is kept");
    // A Markdown link: from the linking note's folder, `..` applied.
    assert_eq!(made(LinkKind::Markdown, "Idea", "Work/2026/a.md").as_deref(), Some("Work/2026/Idea.md"));
    assert_eq!(made(LinkKind::Markdown, "../Idea.md", "Work/2026/a.md").as_deref(), Some("Work/Idea.md"));
    assert_eq!(made(LinkKind::Markdown, "Idea", "a.md").as_deref(), Some("Idea.md"));
    // Nothing named, or a place outside the vault: no note.
    assert_eq!(made(LinkKind::Wiki, "  ", "a.md"), None);
    assert_eq!(made(LinkKind::Markdown, "../../Idea", "Work/a.md"), None);
    assert_eq!(linked_note_path(&link(LinkKind::Wiki, "Idea", None), "a.md", "").as_deref(), Some("Idea.md"), "no folder of new notes: the vault's top");
    assert_eq!((normalize_path("a/./b//c/../d").as_deref(), normalize_path(".."), normalize_path("./")), (Some("a/b/d"), None, None));
}
