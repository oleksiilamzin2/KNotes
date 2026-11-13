use knotes_app::{Moves, plan_relinks};
use knotes_core::{BackLink, BoxError, LinkGraph, LinkResolver, NoteParser, OutLink};
use knotes_index::LinkTargets;
use knotes_md::MarkdownParser;

/// Notes in memory, links resolved as the index would.
struct Vault(Vec<(&'static str, &'static str)>);

impl Vault {
    fn targets(&self) -> LinkTargets {
        LinkTargets::new(self.0.iter().map(|n| n.0))
    }
    fn read(&self, path: &str) -> Option<String> {
        self.0.iter().find(|n| n.0 == path).map(|n| n.1.to_owned())
    }
}

impl LinkGraph for Vault {
    fn links_from(&self, path: &str) -> Result<Vec<OutLink>, BoxError> {
        let targets = self.targets();
        let text = self.read(path).unwrap_or_default();
        Ok(MarkdownParser::new().parse(&text).links.into_iter().map(|link| OutLink { resolved: targets.resolve(link.kind, &link.target, path), link }).collect())
    }
    fn backlinks(&self, path: &str) -> Result<Vec<BackLink>, BoxError> {
        let mut back = Vec::new();
        for (source, _) in &self.0 {
            for out in self.links_from(source)?.into_iter().filter(|l| l.resolved.as_deref() == Some(path) && source != &path) {
                back.push(BackLink { source: source.to_string(), link: out.link });
            }
        }
        Ok(back)
    }
}

fn plan(vault: &Vault, moves: &[(&str, &str)]) -> knotes_app::Relinks {
    let moves: Moves = moves.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let after = LinkTargets::new(vault.0.iter().map(|n| moves.get(n.0).map_or(n.0, String::as_str)));
    plan_relinks(&moves, vault, &|p| vault.read(p), &MarkdownParser::new(), &vault.targets(), &after).unwrap()
}

#[test]
fn renaming_a_note_edits_the_notes_that_link_to_it() {
    let vault = Vault(vec![("a.md", "see [[Old]]"), ("b.md", "[x](Old.md) and [[Old|it]]"), ("Old.md", "body [[a]]"), ("c.md", "nothing")]);
    let plan = plan(&vault, &[("Old.md", "New.md")]);
    let mut edits = plan.edits.clone();
    edits.sort();
    assert_eq!(edits, [("a.md".to_owned(), "see [[New]]".to_owned()), ("b.md".to_owned(), "[x](New.md) and [[New|it]]".to_owned())]);
    assert_eq!((plan.links, plan.edits.len()), (3, 2));
}

#[test]
fn moving_a_folder_edits_notes_inside_and_outside_it_under_their_new_paths() {
    let vault = Vault(vec![("old/a.md", "[up](../c.md) [[old/b]]"), ("old/b.md", "x"), ("c.md", "[[old/a]] [a](old/a.md)")]);
    let plan = plan(&vault, &[("old/a.md", "new/deep/a.md"), ("old/b.md", "new/deep/b.md")]);
    let mut edits = plan.edits.clone();
    edits.sort();
    assert_eq!(
        edits,
        [
            ("c.md".to_owned(), "[[new/deep/a]] [a](new/deep/a.md)".to_owned()),
            ("new/deep/a.md".to_owned(), "[up](../../c.md) [[new/deep/b]]".to_owned()),
        ]
    );
    assert_eq!(plan.links, 4);
}

#[test]
fn nothing_to_edit_when_no_link_changes() {
    let vault = Vault(vec![("a.md", "[[Other]]"), ("Old.md", "x"), ("Other.md", "y")]);
    let plan = plan(&vault, &[("Old.md", "New.md")]);
    assert!(plan.edits.is_empty() && plan.links == 0);
}
