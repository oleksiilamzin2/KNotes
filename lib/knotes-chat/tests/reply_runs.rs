use knotes_chat::{Run, RunLink, RunStyle, reply_runs};

fn text(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

fn run<'a>(runs: &'a [Run], text: &str) -> &'a Run {
    runs.iter().find(|r| r.text == text).unwrap_or_else(|| panic!("no run {text:?} in {runs:?}"))
}

#[test]
fn inline_styles_become_runs() {
    let runs = reply_runs("Hello **world**, *this* and `code`.");
    assert_eq!(text(&runs), "Hello world, this and code.");
    assert_eq!(run(&runs, "Hello ").style, RunStyle::default());
    assert!(run(&runs, "world").style.bold);
    assert!(run(&runs, "this").style.italic);
    assert!(run(&runs, "code").style.code);
}

#[test]
fn wiki_links_point_at_notes_and_web_links_at_the_web() {
    let runs = reply_runs("See [[Projects/Radar]], [[budget.pdf#page=3|the budget]] and [the site](https://example.org).");
    assert_eq!(text(&runs), "See Projects/Radar, the budget and the site.");
    assert_eq!(run(&runs, "Projects/Radar").link, Some(RunLink::Note("Projects/Radar".into())));
    assert_eq!(run(&runs, "the budget").link, Some(RunLink::Note("budget.pdf#page=3".into())));
    assert_eq!(run(&runs, "the site").link, Some(RunLink::Web("https://example.org".into())));
    assert_eq!(run(&runs, "See ").link, None);
}

#[test]
fn blocks_are_set_apart_lists_marked_and_code_kept_as_written() {
    let runs = reply_runs("# Radar\n\nStarts in May.\n\n- one\n- **two**\n\n1. first\n2. second\n\n```rust\nfn main() {}\n```\n\nThe end.");
    assert_eq!(text(&runs), "Radar\n\nStarts in May.\n\n• one\n• two\n\n1. first\n2. second\n\nfn main() {}\n\nThe end.");
    assert_eq!(run(&runs, "Radar").style.heading, 1);
    assert!(run(&runs, "two").style.bold);
    assert!(run(&runs, "fn main() {}").style.code_block);
}

#[test]
fn a_reply_cut_mid_stream_still_renders() {
    assert_eq!(text(&reply_runs("Half a **bold")), "Half a **bold");
    assert_eq!(text(&reply_runs("")), "");
}
