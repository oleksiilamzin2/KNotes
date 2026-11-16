use knotes_editor::embedded_part;

const NOTE: &str = "---\ntags: [x]\n---\n# Title\n\nIntro.\n\n## Part\n\nPart text.\n\n### Sub\n\nSub text.\n\n## Next\n\nFirst line\nNext text. ^blk\n\n- one\n- two ^item\n";

fn part(subpath: Option<&str>) -> Option<&'static str> {
    embedded_part(NOTE, subpath).map(|r| &NOTE[r])
}

#[test]
fn an_embed_shows_the_note_without_its_frontmatter_or_a_section_or_a_block() {
    assert_eq!(part(None), Some("# Title\n\nIntro.\n\n## Part\n\nPart text.\n\n### Sub\n\nSub text.\n\n## Next\n\nFirst line\nNext text. ^blk\n\n- one\n- two ^item"));
    assert_eq!(part(Some("part")), Some("## Part\n\nPart text.\n\n### Sub\n\nSub text."), "to the next heading as high, case ignored");
    assert_eq!(part(Some("^blk")), Some("First line\nNext text."), "its paragraph, without the anchor");
    assert_eq!(part(Some("^item")), Some("- two"), "a list item alone");
    assert_eq!(part(Some("missing")), None);
    assert_eq!(embedded_part("plain", None).map(|r| &"plain"[r]), Some("plain"));
}
