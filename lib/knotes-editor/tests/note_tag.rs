use knotes_editor::{with_tag, without_tag};

#[test]
fn a_tag_goes_into_the_frontmatter_in_the_form_its_tags_have() {
    assert_eq!(with_tag("Текст нотатки.", "no-ai"), "---\ntags: [no-ai]\n---\nТекст нотатки.", "a frontmatter made");
    assert_eq!(with_tag("---\ntitle: X\n---\nТекст", "no-ai"), "---\ntitle: X\ntags: [no-ai]\n---\nТекст", "a key added");
    assert_eq!(with_tag("---\ntags: [дім, гроші]\n---\nx", "no-ai"), "---\ntags: [дім, гроші, no-ai]\n---\nx");
    assert_eq!(with_tag("---\ntags: []\n---\nx", "no-ai"), "---\ntags: [no-ai]\n---\nx");
    assert_eq!(with_tag("---\ntags:\n  - дім\n  - гроші\nother: 1\n---\nx", "no-ai"), "---\ntags:\n  - дім\n  - гроші\n  - no-ai\nother: 1\n---\nx");
    assert_eq!(with_tag("---\ntags: дім гроші\n---\nx", "no-ai"), "---\ntags: [дім, гроші, no-ai]\n---\nx", "words written as a list");
}

#[test]
fn a_note_with_the_tag_already_stays_as_it_is() {
    for text in ["Текст #no-ai тут.", "---\ntags: [no-ai]\n---\nx", "---\ntags:\n  - \"#no-ai\"\n---\nx"] {
        assert_eq!(with_tag(text, "no-ai"), text);
    }
}

#[test]
fn a_tag_goes_out_of_the_frontmatter_and_the_text() {
    assert_eq!(without_tag("---\ntags: [no-ai]\n---\nТекст", "no-ai"), "Текст", "the frontmatter with its last key");
    assert_eq!(without_tag("---\ntitle: X\ntags: [дім, no-ai]\n---\nx", "no-ai"), "---\ntitle: X\ntags: [дім]\n---\nx");
    assert_eq!(without_tag("---\ntags:\n  - дім\n  - no-ai\n---\nx", "no-ai"), "---\ntags:\n  - дім\n---\nx");
    assert_eq!(without_tag("---\ntitle: X\ntags:\n  - no-ai\n---\nx", "no-ai"), "---\ntitle: X\n---\nx", "the key with its last item");
    assert_eq!(without_tag("---\ntags: дім no-ai\n---\nx", "no-ai"), "---\ntags: дім\n---\nx");
    assert_eq!(without_tag("Мій щоденник #no-ai про все.\nКінець #no-ai", "no-ai"), "Мій щоденник про все.\nКінець");
    assert_eq!(without_tag("Код `#no-ai` лишається.", "no-ai"), "Код `#no-ai` лишається.", "a hash in code is no tag");
    let round = "---\ntags: [дім]\n---\nТекст.";
    assert_eq!(without_tag(&with_tag(round, "no-ai"), "no-ai"), round);
}

#[test]
fn a_tag_under_it_counts_as_the_tag() {
    assert_eq!(with_tag("Щоденник #no-ai/особисте", "no-ai"), "Щоденник #no-ai/особисте");
    assert_eq!(without_tag("---\ntags: [no-ai/особисте, дім]\n---\nЩоденник #NO-AI", "no-ai"), "---\ntags: [дім]\n---\nЩоденник");
    assert_eq!(without_tag("#no-airplane лишається", "no-ai"), "#no-airplane лишається");
}
