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

#[test]
fn a_link_of_another_scheme_than_the_web_stays_text() {
    // A note can lead the model to put any link in its reply: one click opened it in its app.
    let runs = reply_runs("[report](file:///etc/hosts), [chat](slack://open), [mail](mailto:a@b.c), [site](HTTPS://example.org), [[Radar]]");
    // Text without a link joins the words next to it.
    for word in ["report", "chat"] {
        assert!(runs.iter().filter(|r| r.text.contains(word)).all(|r| r.link.is_none()), "{word}: {runs:?}");
    }
    assert_eq!(run(&runs, "mail").link, Some(RunLink::Web("mailto:a@b.c".into())));
    assert_eq!(run(&runs, "site").link, Some(RunLink::Web("HTTPS://example.org".into())));
    assert_eq!(run(&runs, "Radar").link, Some(RunLink::Note("Radar".into())));
}

#[test]
fn formulas_in_dollars_or_latex_brackets_become_runs_of_their_source() {
    let runs = reply_runs(r"For \(x\ne a\) the slope $\frac{f(x)-f(a)}{x-a}$ and `\(code\)`, $5 and $6.");
    let formulas: Vec<&str> = runs.iter().filter(|r| r.style.math).map(|r| r.text.as_str()).collect();
    assert_eq!(formulas, [r"x\ne a", r"\frac{f(x)-f(a)}{x-a}"]);
    assert!(runs.iter().all(|r| !r.style.display));
    assert!(run(&runs, r"\(code\)").style.code, "code keeps its brackets: {runs:?}");
    assert!(text(&runs).ends_with(", $5 and $6."), "prices are no formula: {runs:?}");

    let runs = reply_runs("So:\n\n\\[\nf'(a)=2a\n\\]\n\nand $$x^2$$$y$");
    let formulas: Vec<(&str, bool)> = runs.iter().filter(|r| r.style.math).map(|r| (r.text.as_str(), r.style.display)).collect();
    assert_eq!(formulas, [("f'(a)=2a", true), ("x^2", true), ("y", false)]);
}
