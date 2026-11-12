use knotes_editor::{Edit, byte_offset};

#[test]
fn utf16_offsets_map_back_to_byte_offsets() {
    let src = "aї😀b";
    assert_eq!([0, 1, 2, 4, 5].map(|u| byte_offset(src, u)), [0, 1, 3, 7, 8]);
    assert_eq!(byte_offset(src, 99), src.len(), "past the end clamps");
}

#[test]
fn an_edit_applies_to_a_rust_copy_of_the_text() {
    let mut text = String::from("Привіт, світе 😀!");
    // "світе" starts at UTF-16 offset 8; replace it with "друже".
    Edit { location: 8, removed: 5, inserted: 5 }.apply(&mut text, "друже");
    assert_eq!(text, "Привіт, друже 😀!");
    Edit { location: 16, removed: 0, inserted: 1 }.apply(&mut text, "?");
    assert_eq!(text, "Привіт, друже 😀?!");
}
