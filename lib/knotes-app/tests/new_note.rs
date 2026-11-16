use knotes_app::{note_file_stem, unique_note_path};

#[test]
fn the_first_line_becomes_a_file_name_without_markup() {
    assert_eq!(note_file_stem("# Список покупок").as_deref(), Some("Список покупок"));
    assert_eq!(note_file_stem("**Bold** and _it_ `code`").as_deref(), Some("Bold and it code"));
    assert_eq!(note_file_stem("- [ ] task one").as_deref(), Some("task one"));
    assert_eq!(note_file_stem("> quoted line").as_deref(), Some("quoted line"));
    assert_eq!(note_file_stem("1. first").as_deref(), Some("first"));
}

#[test]
fn characters_that_break_files_or_links_are_left_out() {
    assert_eq!(note_file_stem("Plan: 2026/11 [[x]] #tag? ^id |a|").as_deref(), Some("Plan 2026-11 x tag id a"));
    assert_eq!(note_file_stem("...hidden.").as_deref(), Some("hidden"), "no dot files, no trailing dots");
    assert_eq!(note_file_stem("   "), None);
    assert_eq!(note_file_stem("## "), None);
    assert_eq!(note_file_stem("[[]]"), None);
}

#[test]
fn long_lines_are_cut_at_a_word() {
    let stem = note_file_stem(&"слово ".repeat(40)).unwrap();
    assert!(stem.chars().count() <= 100, "{stem}");
    assert!(stem.ends_with("слово"), "{stem}");
}

#[test]
fn names_taken_get_a_number() {
    let taken = ["Вхідні/Без назви.md", "Вхідні/Без назви 1.md"];
    let exists = |p: &str| taken.contains(&p);
    assert_eq!(unique_note_path("Вхідні", "Без назви", exists), "Вхідні/Без назви 2.md");
    assert_eq!(unique_note_path("", "Ідея", exists), "Ідея.md");
}

#[test]
fn a_new_folder_gets_a_name_no_folder_beside_it_has() {
    use knotes_app::unique_folder_path;
    assert_eq!(unique_folder_path("", "Нова тека", |_| false), "Нова тека");
    assert_eq!(unique_folder_path("Work", "Нова тека", |p| p == "Work/Нова тека"), "Work/Нова тека 2");
    assert_eq!(unique_folder_path("Work", "Нова тека", |p| p == "Work/Нова тека" || p == "Work/Нова тека 2"), "Work/Нова тека 3");
}

#[test]
fn in_a_space_a_new_note_goes_to_the_inbox_if_it_is_the_space_s_else_to_its_first_folder() {
    use knotes_app::{SpaceFolders, folder_tree, space_notes_folder};
    use knotes_spaces::Spaces;
    let tree = folder_tree(["Work/a.md", "Areas/Job/b.md", "Вхідні/c.md"]);
    let spaces = Spaces::from_texts(&[(String::new(), r#"{"spaces": [{"id": "work"}, {"id": "empty"}]}"#.into()), ("Work".into(), r#"{"space": "work"}"#.into()), ("Areas/Job".into(), r#"{"space": "work"}"#.into())]);
    let work = SpaceFolders::new(&tree, &spaces, "work");
    assert_eq!(space_notes_folder("Вхідні", &work, &tree), "Areas/Job", "the first of its folders at the top");
    let with_inbox = Spaces::from_texts(&[(String::new(), r#"{"spaces": [{"id": "work"}]}"#.into()), ("Work".into(), r#"{"space": "work"}"#.into()), ("Вхідні".into(), r#"{"space": "work"}"#.into())]);
    assert_eq!(space_notes_folder("Вхідні", &SpaceFolders::new(&tree, &with_inbox, "work"), &tree), "Вхідні");
    assert_eq!(space_notes_folder("Вхідні", &SpaceFolders::new(&tree, &spaces, "empty"), &tree), "Вхідні", "a space with no folder: the inbox");
}

#[test]
fn a_moved_file_keeps_its_extension() {
    use knotes_app::unique_file_path;
    assert_eq!(unique_file_path("Фінанси", "Поліс.pdf", |_| false), "Фінанси/Поліс.pdf");
    assert_eq!(unique_file_path("Фінанси", "Поліс.pdf", |p| p == "Фінанси/Поліс.pdf"), "Фінанси/Поліс 1.pdf");
    assert_eq!(unique_file_path("", "Note.md", |p| p == "Note.md"), "Note 1.md");
    assert_eq!(unique_file_path("A", "Note.MD", |_| false), "A/Note.MD");
    assert_eq!(unique_file_path("A", "README", |_| false), "A/README");
    assert_eq!(unique_file_path("A", ".hidden", |_| false), "A/.hidden");
}
