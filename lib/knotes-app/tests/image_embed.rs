use knotes_app::{embed_images, pasted_image_name};

fn paths(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn a_pasted_image_is_named_as_obsidian_names_it() {
    assert_eq!(pasted_image_name("20261113114500"), "Pasted image 20261113114500.png");
}

#[test]
fn images_are_embedded_by_name_each_on_its_own_line() {
    let embeds = embed_images(&paths(&[".knotes/attachments/a.png", ".knotes/attachments/b 1.jpg"]), "", "", |_| true);
    assert_eq!(embeds, "![[a.png]]\n![[b 1.jpg]]");
}

#[test]
fn an_image_whose_name_another_file_has_is_embedded_by_its_path() {
    let embeds = embed_images(&paths(&[".knotes/attachments/photo.jpg"]), "", "", |_| false);
    assert_eq!(embeds, "![[.knotes/attachments/photo.jpg]]");
}

#[test]
fn text_beside_the_cursor_moves_to_lines_of_its_own() {
    let one = paths(&[".knotes/attachments/a.png"]);
    assert_eq!(embed_images(&one, "Look: ", "", |_| true), "\n![[a.png]]");
    assert_eq!(embed_images(&one, "", " and more", |_| true), "![[a.png]]\n");
    assert_eq!(embed_images(&one, "x", "y", |_| true), "\n![[a.png]]\n");
    assert_eq!(embed_images(&one, "  ", "  ", |_| true), "![[a.png]]", "white space is no text");
}
