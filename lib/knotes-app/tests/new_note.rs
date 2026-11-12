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
