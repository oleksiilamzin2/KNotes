use knotes_editor::{ListKind, Style, highlight, utf16_offsets};

/// Runs other than the title (a note's first line is one, tested on its own).
fn styles(src: &str) -> Vec<(Style, &str)> {
    highlight(src).runs.into_iter().filter(|r| r.style != Style::Title).map(|r| (r.style, &src[r.range])).collect()
}

fn markup(src: &str) -> Vec<&str> {
    highlight(src).markup.into_iter().map(|r| &src[r]).collect()
}

#[test]
fn headings_hide_their_hashes() {
    // Below the first line, which is the note's title.
    assert_eq!(styles("intro\n\n## Title\n"), [(Style::Heading(2), "## Title")]);
    assert_eq!(markup("intro\n\n## Title\n"), ["## "]);
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
    assert_eq!(styles(src).into_iter().map(|s| s.0).collect::<Vec<_>>(), [Style::CodeBlock, Style::Fence, Style::Fence]);
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

#[test]
fn whole_highlight_converts_to_utf16_ranges() {
    let src = "ж **б** [[N|a]]";
    let h = highlight(src).to_utf16(src);
    let utf16: Vec<u16> = src.encode_utf16().collect();
    let text = |r: std::ops::Range<usize>| String::from_utf16(&utf16[r]).unwrap();
    let runs: Vec<(Style, String)> = h.runs.iter().map(|r| (r.style, text(r.range.clone()))).collect();
    assert_eq!(runs, [(Style::Title, src.to_owned()), (Style::Strong, "**б**".to_owned()), (Style::WikiLink, "[[N|a]]".to_owned())]);
    assert_eq!(h.markup.iter().map(|r| text(r.clone())).collect::<Vec<_>>(), ["**", "**", "[[N|", "]]"]);
}

#[test]
fn markup_takes_the_style_of_the_innermost_run_around_it() {
    let src = "# Title **b**\n\nplain _i_ [[Note|alias]]";
    let h = highlight(src);
    let owners: Vec<(&str, Option<Style>)> = h.markup.iter().zip(h.markup_owners()).map(|(m, o)| (&src[m.clone()], o)).collect();
    assert_eq!(owners[0], ("# ", Some(Style::Title)), "the first line is the title");
    assert_eq!(owners[1], ("**", Some(Style::Strong)));
    assert_eq!(owners[2], ("**", Some(Style::Strong)));
    assert!(owners[3..].iter().all(|(_, o)| matches!(o, Some(Style::Emphasis | Style::WikiLink))), "{owners:?}");
}

#[test]
fn markup_owners_match_a_plain_search_on_a_large_note() {
    let src: String = (0..2000).map(|i| format!("## H{i} *x*\n\na **b _c_** [[N{i}|n]] `k` ~~s~~ [l](u) #t{i}\n\n")).collect();
    let h = highlight(&src);
    let plain: Vec<Option<Style>> = h
        .markup
        .iter()
        .map(|m| h.runs.iter().rev().find(|r| r.range.start <= m.start && m.end <= r.range.end).map(|r| r.style))
        .collect();
    assert_eq!(h.markup_owners(), plain);
}

#[test]
fn links_carry_their_targets_over_the_whole_link_source() {
    use knotes_core::LinkKind;
    let src = "Їжак [[A/Note#H|x]] and [t](b%20c.md) and [w](https://e.com)";
    let h = highlight(src).to_utf16(src);
    let utf16: Vec<u16> = src.encode_utf16().collect();
    let shown: Vec<(String, LinkKind, &str, Option<&str>)> = h
        .links
        .iter()
        .map(|l| (String::from_utf16(&utf16[l.range.clone()]).unwrap(), l.link.kind, l.link.target.as_str(), l.link.subpath.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            ("[[A/Note#H|x]]".to_owned(), LinkKind::Wiki, "A/Note", Some("H")),
            ("[t](b%20c.md)".to_owned(), LinkKind::Markdown, "b c.md", None),
            ("[w](https://e.com)".to_owned(), LinkKind::External, "https://e.com", None),
        ]
    );
}

#[test]
fn formulas_carry_their_source_and_kind() {
    let src = "Energy $E = mc^2$ here.\n\n$$\n\\frac{a}{b}\n$$\n\nPrices $5 and $10 stay text.";
    let h = highlight(src);
    let formulas: Vec<(&str, &str, bool)> = h.formulas.iter().map(|f| (&src[f.range.clone()], f.tex.as_str(), f.display)).collect();
    assert_eq!(formulas, [("$E = mc^2$", "E = mc^2", false), ("$$\n\\frac{a}{b}\n$$", "\\frac{a}{b}", true)]);
    assert!(styles(src).contains(&(Style::Math, "$E = mc^2$")), "{:?}", styles(src));
    assert!(styles(src).contains(&(Style::Math, "$$\n\\frac{a}{b}\n$$")));
    let utf16 = highlight("Їж $x$").to_utf16("Їж $x$");
    assert_eq!(utf16.formulas[0].range, 3..6);
}

fn list_marks(src: &str) -> Vec<(ListKind, u8, &str)> {
    highlight(src)
        .runs
        .into_iter()
        .filter_map(|r| match r.style {
            Style::ListMarker { kind, depth } => Some((kind, depth, &src[r.range])),
            _ => None,
        })
        .collect()
}

#[test]
fn list_items_mark_their_bullet_or_number_with_the_space_after_it() {
    assert_eq!(list_marks("- a\n* b\n"), [(ListKind::Bullet, 0, "- "), (ListKind::Bullet, 0, "* ")]);
    assert_eq!(list_marks("+ a"), [(ListKind::Bullet, 0, "+ ")]);
    assert_eq!(list_marks("1. a\n2. b\n"), [(ListKind::Ordered, 0, "1. "), (ListKind::Ordered, 0, "2. ")]);
    assert_eq!(list_marks("10) a"), [(ListKind::Ordered, 0, "10) ")]);
}

#[test]
fn nested_list_items_know_their_depth_and_leave_the_indentation_out() {
    let src = "- a\n  - b\n    1. c\n- d";
    assert_eq!(list_marks(src), [(ListKind::Bullet, 0, "- "), (ListKind::Bullet, 1, "- "), (ListKind::Ordered, 2, "1. "), (ListKind::Bullet, 0, "- ")]);
    let at: Vec<usize> = highlight(src).runs.iter().map(|r| r.range.start).collect();
    assert_eq!(at, [0, 6, 14, 19]);
    // Obsidian indents with tabs.
    assert_eq!(list_marks("- one\n\t- two\n\t\t- three"), [(ListKind::Bullet, 0, "- "), (ListKind::Bullet, 1, "- "), (ListKind::Bullet, 2, "- ")]);
}

#[test]
fn tasks_mark_their_box_and_done_ones_their_text() {
    let src = "- [ ] todo\n- [x] done it\n";
    assert_eq!(list_marks(src), [(ListKind::Task { checked: false }, 0, "- [ ] "), (ListKind::Task { checked: true }, 0, "- [x] ")]);
    assert_eq!(styles(src).into_iter().filter(|s| s.0 == Style::Done).collect::<Vec<_>>(), [(Style::Done, "done it")]);
}

#[test]
fn list_items_in_a_quote_are_marked_too() {
    assert_eq!(list_marks("> - a"), [(ListKind::Bullet, 0, "- ")]);
}

fn quote_marks(src: &str) -> Vec<(u8, &str)> {
    highlight(src)
        .runs
        .into_iter()
        .filter_map(|r| match r.style {
            Style::QuoteMarker { depth } => Some((depth, &src[r.range])),
            _ => None,
        })
        .collect()
}

#[test]
fn quote_lines_mark_their_prefix_with_its_depth() {
    assert_eq!(quote_marks("> said\n> twice\n"), [(1, "> "), (1, "> ")]);
    assert_eq!(quote_marks("> one\n> > two\n>three"), [(1, "> "), (2, "> > "), (1, ">")]);
    assert_eq!(quote_marks("  > indented"), [(1, "  > ")]);
}

#[test]
fn a_lazy_quote_line_has_no_prefix_and_a_quote_in_a_list_starts_at_its_marker() {
    assert_eq!(quote_marks("> a\nlazy\n"), [(1, "> ")]);
    let src = "- > in list";
    assert_eq!(quote_marks(src), [(1, "> ")]);
    assert_eq!(highlight(src).runs.iter().find(|r| matches!(r.style, Style::QuoteMarker { .. })).unwrap().range.start, 2);
}

fn fences(src: &str) -> Vec<&str> {
    highlight(src).runs.into_iter().filter(|r| r.style == Style::Fence).map(|r| &src[r.range]).collect()
}

#[test]
fn fenced_code_blocks_mark_their_fence_lines_and_their_whole_range() {
    let src = "text\n\n```rust\nlet x = 1;\n```\n\nend";
    assert_eq!(fences(src), ["```rust", "```"]);
    let blocks: Vec<&str> = highlight(src).code_blocks.into_iter().map(|r| &src[r]).collect();
    assert_eq!(blocks, ["```rust\nlet x = 1;\n```"]);
    assert_eq!(fences("~~~\ncode\n~~~"), ["~~~", "~~~"]);
}

#[test]
fn an_unclosed_or_indented_code_block_has_no_closing_fence() {
    assert_eq!(fences("```\ncode\nmore"), ["```"]);
    let src = "para\n\n    indented code\n";
    assert!(fences(src).is_empty());
    assert_eq!(highlight(src).code_blocks.into_iter().map(|r| &src[r]).collect::<Vec<_>>(), ["indented code"]);
}

fn title(src: &str) -> Option<&str> {
    highlight(src).title.map(|r| &src[r])
}

#[test]
fn the_first_line_of_a_heading_or_paragraph_is_the_title() {
    assert_eq!(title("# My note\n\ntext"), Some("# My note"));
    assert_eq!(title("First line\nstill the paragraph\n\nmore"), Some("First line"));
    assert_eq!(title("\n\nAfter blank lines"), Some("After blank lines"));
    assert_eq!(title("---\ntags: [a]\n---\n\n## Below frontmatter\n"), Some("## Below frontmatter"));
    let src = "# My note\n\ntext";
    let runs: Vec<Style> = highlight(src).runs.iter().map(|r| r.style).collect();
    assert_eq!(runs, [Style::Title], "the heading run is the title run");
    let src = "[[Link]] in the title";
    let runs: Vec<Style> = highlight(src).runs.iter().map(|r| r.style).collect();
    assert_eq!(runs, [Style::Title, Style::WikiLink], "runs inside show over the title");
}

#[test]
fn a_note_starting_with_a_list_code_or_quote_has_no_title() {
    assert_eq!(title("- item\n"), None);
    assert_eq!(title("```\ncode\n```\n"), None);
    assert_eq!(title("> quote\n"), None);
    assert_eq!(title(""), None);
}
