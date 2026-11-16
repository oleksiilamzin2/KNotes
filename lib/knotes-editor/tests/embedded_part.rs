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

#[test]
fn an_edited_part_goes_back_where_it_was_or_where_it_is_now() {
    use knotes_editor::splice_part;
    let was = "# A\n\n## Part\n\nOld.\n\n## Next\n\nN.\n";
    let range = embedded_part(was, Some("Part")).unwrap();
    // The note as last read: the part's place is known, though its heading changed in the edit.
    let (written, new) = splice_part(was, was, range.clone(), Some("Part"), "## Renamed\n\nNew.").unwrap();
    assert_eq!(written, "# A\n\n## Renamed\n\nNew.\n\n## Next\n\nN.\n");
    assert_eq!(&written[new.clone()], "## Renamed\n\nNew.");
    // Changed elsewhere since: the part is found again.
    let now = format!("Intro line.\n{was}");
    let (written, new) = splice_part(&now, was, range.clone(), Some("Part"), "## Part\n\nNewer.").unwrap();
    assert_eq!(written, "Intro line.\n# A\n\n## Part\n\nNewer.\n\n## Next\n\nN.\n");
    assert_eq!(&written[new], "## Part\n\nNewer.");
    // Gone elsewhere: nothing written.
    assert!(splice_part("# A\n\nno part now\n", was, range, Some("Part"), "x").is_none());
    // A whole note keeps its frontmatter.
    let whole = "---\ntags: [x]\n---\nBody.\n";
    let part = embedded_part(whole, None).unwrap();
    let (written, _) = splice_part(whole, whole, part, None, "Body, longer.").unwrap();
    assert_eq!(written, "---\ntags: [x]\n---\nBody, longer.\n");
}
