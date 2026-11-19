use knotes_vault::start_vault;

#[test]
fn an_empty_folder_is_given_knotes_folders_and_one_with_files_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let new = dir.path().join("Нова/база");
    assert!(start_vault(&new, "Вхідні").unwrap(), "made, with the folders over it");
    assert!(new.join(".knotes").is_dir() && new.join("Вхідні").is_dir());
    assert!(!start_vault(&new, "Вхідні").unwrap(), "started already");
    let finder = dir.path().join("finder");
    std::fs::create_dir(&finder).unwrap();
    std::fs::write(finder.join(".DS_Store"), "").unwrap();
    assert!(start_vault(&finder, "Вхідні").unwrap(), "Finder's own file aside, it is empty");
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    std::fs::write(notes.join("a.md"), "a").unwrap();
    assert!(!start_vault(&notes, "Вхідні").unwrap());
    assert_eq!(std::fs::read_dir(&notes).unwrap().count(), 1, "nothing added to a folder of notes");
}
