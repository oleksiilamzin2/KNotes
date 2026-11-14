use knotes_app::{Mention, backlink_mentions, make_link, outgoing_links, unlinked_mentions};
use knotes_core::NoteParser;
use knotes_index::LinkTargets;
use knotes_md::MarkdownParser;

fn targets() -> LinkTargets {
    LinkTargets::new(["Plan.md", "Docs/Report.md", "Other.md", "doc.pdf"].into_iter())
}

/// Each mention's line with the mention in brackets.
fn marked(mentions: &[Mention]) -> Vec<String> {
    mentions.iter().map(|m| format!("{}[{}]{}", &m.line[..m.mark.start], &m.line[m.mark.clone()], &m.line[m.mark.end..])).collect()
}

#[test]
fn a_note_s_links_to_the_document_come_with_the_line_around_each() {
    let text = "# Notes\n\nSee [[Plan]] for the dates.\n- also [the plan](Plan.md) and [[Other]]\n![[doc.pdf]]";
    let parsed = MarkdownParser::new().parse(text);
    let found = backlink_mentions("Plan.md", "Daily.md", text, &parsed, &targets());
    assert_eq!(marked(&found), ["See [[[Plan]]] for the dates.", "- also [[the plan](Plan.md)] and [[Other]]"]);
    assert_eq!(&text[found[0].range.clone()], "[[Plan]]");
    let pdf = backlink_mentions("doc.pdf", "Daily.md", text, &parsed, &targets());
    assert_eq!(pdf.len(), 1, "an embed links too");
}

#[test]
fn unlinked_mentions_are_the_name_as_a_word_outside_links_ignoring_case() {
    let text = "The plan is ready. [[Plan]] links. Planning is not it, nor is airplan.\nPLAN again";
    let parsed = MarkdownParser::new().parse(text);
    let found = unlinked_mentions("Plan", text, &parsed);
    assert_eq!(marked(&found), ["The [plan] is ready. [[Plan]] links. Planning is not it, nor is airplan.", "[PLAN] again"]);
}

#[test]
fn outgoing_links_are_grouped_by_target_in_order_and_missing_ones_have_none() {
    let text = "[[Other]] then [[Missing note]] and [[Other|again]] and https://example.com";
    let parsed = MarkdownParser::new().parse(text);
    let out = outgoing_links("Daily.md", text, &parsed, &targets());
    let shown: Vec<(&str, Option<&str>, usize)> = out.iter().map(|o| (o.label.as_str(), o.resolved.as_deref(), o.mentions.len())).collect();
    assert_eq!(shown, [("Other", Some("Other.md"), 2), ("Missing note", None, 1)]);
}

#[test]
fn a_mention_made_a_link_keeps_its_words() {
    assert_eq!(make_link("The plan is ready", 4..8, "Plan"), "The [[Plan|plan]] is ready");
    assert_eq!(make_link("See Plan now", 4..8, "Plan"), "See [[Plan]] now");
}

#[test]
fn a_long_line_is_cut_around_the_link() {
    let text = format!("{} [[Plan]] {}", "word ".repeat(60), "tail ".repeat(60));
    let parsed = MarkdownParser::new().parse(&text);
    let found = backlink_mentions("Plan.md", "Daily.md", &text, &parsed, &targets());
    assert!(found[0].line.chars().count() < 200 && found[0].line.starts_with('…') && found[0].line.ends_with('…'), "{}", found[0].line);
    assert_eq!(&found[0].line[found[0].mark.clone()], "[[Plan]]");
}
