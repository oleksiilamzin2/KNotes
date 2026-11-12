use knotes_editor::{DiskChange, Document, conflict_path};

#[test]
fn own_writes_are_ignored() {
    let mut doc = Document::open("hello");
    doc.edited();
    doc.saved("hello world");
    assert_eq!(doc.on_disk("hello world", "hello world!"), DiskChange::Ignore);
}

#[test]
fn outside_change_reloads_when_there_are_no_unsaved_edits() {
    let mut doc = Document::open("v1");
    assert_eq!(doc.on_disk("v2 from outside", "v1"), DiskChange::Reload);
    assert_eq!(doc.on_disk("v2 from outside", "v2 from outside"), DiskChange::Ignore);
}

#[test]
fn outside_change_with_unsaved_edits_is_a_conflict() {
    let mut doc = Document::open("v1");
    doc.edited();
    assert!(doc.is_dirty());
    assert_eq!(doc.on_disk("v2 from outside", "v1 edited"), DiskChange::Conflict);
}

#[test]
fn outside_text_equal_to_the_editor_needs_nothing() {
    let mut doc = Document::open("v1");
    doc.edited();
    assert_eq!(doc.on_disk("same", "same"), DiskChange::Ignore);
    assert!(!doc.is_dirty());
}

#[test]
fn conflict_copies_sit_next_to_the_note() {
    assert_eq!(conflict_path("Folder/Name.md", "2026-11-12 15.30"), "Folder/Name (конфлікт 2026-11-12 15.30).md");
    assert_eq!(conflict_path("n.md", "2026-11-12 15.30"), "n (конфлікт 2026-11-12 15.30).md");
}

#[test]
fn a_new_note_takes_its_name_once_the_first_line_is_finished_or_left() {
    let mut doc = Document::untitled();
    assert!(!doc.wants_name("Іде", false), "the first line is still being typed");
    assert!(doc.wants_name("Ідея\nтекст", false), "Return finished the first line");
    assert!(doc.wants_name("Ідея", true), "leaving the note finishes it too");
    assert!(!doc.wants_name("", true), "nothing to name it after");
    doc.named();
    assert!(!doc.wants_name("Інша назва\n", true), "only once");
    assert!(!Document::open("Existing\n").wants_name("Existing\n", true), "existing notes keep their names");
}
