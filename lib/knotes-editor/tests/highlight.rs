use knotes_editor::{Style, highlight, utf16_offsets};

fn styles(src: &str) -> Vec<(Style, &str)> {
    highlight(src).runs.into_iter().map(|r| (r.style, &src[r.range])).collect()
}

fn markup(src: &str) -> Vec<&str> {
    highlight(src).markup.into_iter().map(|r| &src[r]).collect()
}

#[test]
fn headings_hide_their_hashes() {
    assert_eq!(styles("## Title\n"), [(Style::Heading(2), "## Title")]);
    assert_eq!(markup("## Title\n"), ["## "]);
}

#[test]
fn emphasis_strong_strike_and_code_hide_delimiters() {
    assert_eq!(styles("a **b** c"), [(Style::Strong, "**b**")]);
    assert_eq!(markup("a **b** c"), ["**", "**"]);
    assert_eq!(markup("*i* _j_ ~~s~~ `c`"), ["*", "*", "_", "_", "~~", "~~", "`", "`"]);
    assert_eq!(
        styles("*i* ~~s~~ `c`").into_iter().map(|s| s.0).collect::<Vec<_>>(),
        [Style::Emphasis, Style::Strike, Style::Code]
    );
}

#[test]
fn wikilinks_show_only_the_alias_or_target() {
    assert_eq!(styles("see [[Note|alias]]"), [(Style::WikiLink, "[[Note|alias]]")]);
    assert_eq!(markup("see [[Note|alias]]"), ["[[Note|", "]]"]);
    assert_eq!(markup("[[Note#Part]]"), ["[[", "]]"]);
}

#[test]
fn markdown_links_show_only_their_text() {
    assert_eq!(styles("[text](https://x.y)"), [(Style::Link, "[text](https://x.y)")]);
    assert_eq!(markup("[text](https://x.y)"), ["[", "](https://x.y)"]);
}

#[test]
fn tags_are_styled_but_not_hidden() {
    assert_eq!(styles("plan #todo now"), [(Style::Tag, "#todo")]);
    assert!(markup("plan #todo now").is_empty());
}

#[test]
fn code_blocks_are_one_run_without_inline_styles() {
    let src = "```\n**not bold** [[no]]\n```\n";
    assert_eq!(styles(src).into_iter().map(|s| s.0).collect::<Vec<_>>(), [Style::CodeBlock]);
    assert!(markup(src).is_empty());
}

#[test]
fn ukrainian_text_keeps_byte_ranges_right() {
    assert_eq!(styles("слово **жирне** тут"), [(Style::Strong, "**жирне**")]);
    assert_eq!(markup("# Заголовок"), ["# "]);
}

#[test]
fn byte_offsets_convert_to_utf16() {
    // "ж" is 2 bytes and 1 UTF-16 unit; "😀" is 4 bytes and 2 units.
    let src = "aж😀b";
    assert_eq!(utf16_offsets(src, &[0, 1, 3, 7, 8]), [0, 1, 2, 4, 5]);
}
