use knotes_app::{folder_moves, in_renamed_folder, moved_into, named_after, renamed, renamed_folder};

/// A vault holding `files` (and their folders), names compared as the disk does: any case.
fn vault(files: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |path: &str| files.iter().any(|file| file.to_lowercase() == path.to_lowercase() || file.to_lowercase().starts_with(&format!("{}/", path.to_lowercase())))
}

#[test]
fn a_note_or_a_pdf_renamed_stays_in_its_folder_under_a_free_name() {
    let has = vault(&["Work/Plan.md", "Work/Budget.md", "Work/Budget 1.md", "Work/scan.pdf", "Work/old.pdf", "Top.md"]);
    assert_eq!(renamed("Work/Plan.md", "Roadmap", &has).as_deref(), Some("Work/Roadmap.md"));
    assert_eq!(renamed("Top.md", "# Start: here/now", &has).as_deref(), renamed("Top.md", "Start: here/now", &has).as_deref(), "named as a first line would name it");
    assert_eq!(renamed("Work/Plan.md", "Budget", &has).as_deref(), Some("Work/Budget 2.md"), "the name and its first number are taken");
    assert_eq!(renamed("Work/Plan.md", "plan", &has).as_deref(), Some("Work/plan.md"), "a change of case is the same file renamed");
    assert_eq!(renamed("Work/Plan.md", "Plan", &has), None, "its own name");
    assert_eq!(renamed("Work/Plan.md", "  ", &has), None, "no name");
    // A PDF keeps `.pdf`, written or not.
    assert_eq!(renamed("Work/scan.pdf", "Contract", &has).as_deref(), Some("Work/Contract.pdf"));
    assert_eq!(renamed("Work/scan.pdf", "Contract.PDF", &has).as_deref(), Some("Work/Contract.pdf"));
    assert_eq!(renamed("Work/scan.pdf", "old", &has).as_deref(), Some("Work/old 1.pdf"));
    assert_eq!(renamed("Work/scan.pdf", "Scan", &has).as_deref(), Some("Work/Scan.pdf"));
    assert_eq!((renamed("Work/scan.pdf", "scan.pdf", &has), renamed("Work/scan.pdf", ".pdf", &has)), (None, None));
    // A new note takes its first line's name; its own file is no name taken.
    assert_eq!(named_after("Work/Без назви.md", "# Budget", &has).as_deref(), Some("Work/Budget 2.md"));
    assert_eq!(named_after("Work/Plan.md", "Plan", &has).as_deref(), Some("Work/Plan.md"), "named so already");
    assert_eq!(named_after("Work/Plan.md", "", &has), None);
}

#[test]
fn a_file_moved_keeps_its_name_and_a_folder_renamed_takes_what_is_in_it() {
    let has = vault(&["Work/Plan.md", "Home/Plan.md", "Home/scan.pdf", "Work/2026/Radar.md", "Works/x.md", "Top.md"]);
    assert_eq!(moved_into("Top.md", "Work", &has).as_deref(), Some("Work/Top.md"));
    assert_eq!(moved_into("Work/Plan.md", "Home", &has).as_deref(), Some("Home/Plan 1.md"), "numbered when taken");
    assert_eq!(moved_into("Home/scan.pdf", "", &has).as_deref(), Some("scan.pdf"), "to the vault's root");
    assert_eq!((moved_into("Work/Plan.md", "Work", &has), moved_into("Top.md", "", &has)), (None, None), "there already");
    // A folder renamed beside where it is.
    assert_eq!(renamed_folder("Work/2026", "Архів", &has).as_deref(), Some("Work/Архів"));
    assert_eq!(renamed_folder("Work", "A/B", &has).as_deref(), Some("A-B"), "a name is one folder, not a path");
    assert_eq!(renamed_folder("Work", "work", &has).as_deref(), Some("work"), "a change of case");
    assert_eq!((renamed_folder("Work", "Home", &has), renamed_folder("Work", "Work", &has), renamed_folder("Work", " ", &has)), (None, None, None), "taken, its own, empty");
    // What is in it goes with it; a folder whose name only begins as its does not.
    assert_eq!(in_renamed_folder("Work", "Work", "Job").as_deref(), Some("Job"));
    assert_eq!(in_renamed_folder("Work/2026/Radar.md", "Work", "Job").as_deref(), Some("Job/2026/Radar.md"));
    assert_eq!(in_renamed_folder("Works/x.md", "Work", "Job"), None);
    let mut moves: Vec<(String, String)> = folder_moves(["Work/Plan.md", "Work/2026/Radar.md", "Works/x.md", "Top.md"], "Work", "Job").into_iter().collect();
    moves.sort();
    assert_eq!(moves, [("Work/2026/Radar.md".to_owned(), "Job/2026/Radar.md".to_owned()), ("Work/Plan.md".to_owned(), "Job/Plan.md".to_owned())]);
}
