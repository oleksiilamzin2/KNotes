use knotes_app::rewrite_links;
use knotes_core::{LinkResolver, NoteParser};
use knotes_index::LinkTargets;
use knotes_md::MarkdownParser;

/// The text of note `source` once the note at `from` moved to `to`, in a vault of `paths`
/// (before the move).
fn moved(text: &str, source: &str, from: &str, to: &str, paths: &[&str]) -> String {
    let before = LinkTargets::new(paths.iter().copied());
    let after = LinkTargets::new(paths.iter().map(|p| if *p == from { to } else { p }));
    let links: Vec<_> = MarkdownParser::new()
        .parse(text)
        .links
        .into_iter()
        .filter(|l| before.resolve(l.kind, &l.target, source).as_deref() == Some(from))
        .collect();
    rewrite_links(text, source, &links, from, to, &after)
}

#[test]
fn a_bare_wikilink_takes_the_new_name_keeping_its_heading_alias_and_embed() {
    let paths = ["a.md", "notes/Old.md"];
    let text = "see [[Old]], [[Old#Part|that part]], [[Old#^b1]] and ![[Old]]";
    assert_eq!(moved(text, "a.md", "notes/Old.md", "notes/New.md", &paths), "see [[New]], [[New#Part|that part]], [[New#^b1]] and ![[New]]");
}

#[test]
fn a_bare_name_that_would_open_another_note_gets_the_full_path() {
    // A bare name opens the note with the shortest path (ADR 0009): `New.md` here.
    let paths = ["a.md", "notes/Old.md", "New.md"];
    assert_eq!(moved("[[Old]]", "a.md", "notes/Old.md", "notes/New.md", &paths), "[[notes/New]]");
}

#[test]
fn a_full_path_stays_full_and_md_stays() {
    let paths = ["a.md", "notes/Old.md"];
    let text = "[[notes/Old]] [[notes/Old.md]] [[Old.md]]";
    assert_eq!(moved(text, "a.md", "notes/Old.md", "archive/New.md", &paths), "[[archive/New]] [[archive/New.md]] [[New.md]]");
}

#[test]
fn markdown_links_keep_their_relative_or_root_form() {
    let paths = ["dir/a.md", "dir/Old.md", "x/b.md"];
    let text = "[t](Old.md) [u](Old.md#Part) [v](<Old.md>)";
    assert_eq!(
        moved(text, "dir/a.md", "dir/Old.md", "other/New Name.md", &paths),
        "[t](../other/New%20Name.md) [u](../other/New%20Name.md#Part) [v](<../other/New Name.md>)"
    );
    assert_eq!(moved("[t](dir/Old.md)", "x/b.md", "dir/Old.md", "other/New.md", &paths), "[t](other/New.md)");
}

#[test]
fn other_links_and_text_stay_as_they_are() {
    let paths = ["a.md", "Old.md", "Keep.md"];
    let text = "[[Keep]] [[Old]] [site](https://x.y) Old";
    assert_eq!(moved(text, "a.md", "Old.md", "New.md", &paths), "[[Keep]] [[New]] [site](https://x.y) Old");
}

#[test]
fn a_moved_note_keeps_its_relative_markdown_links_pointing_where_they_did() {
    let paths = ["dir/a.md", "dir/b.md", "x/c.md"];
    let before = LinkTargets::new(paths.iter().copied());
    let text = "[b](b.md) [c](../x/c.md) [root](x/c.md) [self](a.md#h) [[b]] [gone](nowhere.md)";
    let links: Vec<knotes_core::OutLink> = MarkdownParser::new()
        .parse(text)
        .links
        .into_iter()
        .map(|link| knotes_core::OutLink { resolved: before.resolve(link.kind, &link.target, "dir/a.md"), link })
        .collect();
    assert_eq!(
        knotes_app::rebase_links(text, &links, "dir/a.md", "other/sub/a.md"),
        "[b](../../dir/b.md) [c](../../x/c.md) [root](x/c.md) [self](a.md#h) [[b]] [gone](nowhere.md)"
    );
}
