use knotes_editor::set_image_width;

#[test]
fn a_wikilink_embed_gets_the_width_after_a_bar() {
    assert_eq!(set_image_width("![[pic.png]]", 300), "![[pic.png|300]]");
    assert_eq!(set_image_width("![[pic.png|200]]", 300), "![[pic.png|300]]");
    assert_eq!(set_image_width("![[pic.png|200x100]]", 300), "![[pic.png|300]]", "the height follows the width");
    assert_eq!(set_image_width("![[pic.png|a photo]]", 300), "![[pic.png|a photo|300]]", "alt text stays");
    assert_eq!(set_image_width("![[pic.png|a photo|120]]", 300), "![[pic.png|a photo|300]]");
}

#[test]
fn a_markdown_embed_gets_the_width_in_its_alt_text() {
    assert_eq!(set_image_width("![](dir/pic.png)", 300), "![|300](dir/pic.png)");
    assert_eq!(set_image_width("![photo](pic.png)", 300), "![photo|300](pic.png)");
    assert_eq!(set_image_width("![photo|200](pic.png \"title\")", 300), "![photo|300](pic.png \"title\")");
    assert_eq!(set_image_width("![240](pic.png)", 300), "![300](pic.png)");
}
