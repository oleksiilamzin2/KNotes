use knotes_vault::FsVault;

#[test]
fn reviews_are_appended_to_a_log_a_mac_and_read_from_where_one_left() {
    let dir = tempfile::tempdir().unwrap();
    let vault = FsVault::new(dir.path());
    assert!(vault.review_logs().unwrap().is_empty(), "no folder yet");
    vault.append_review("mac-1", "1\tc-1\t3\n").unwrap();
    vault.append_review("mac-1", "2\tc-2\t1\n").unwrap();
    vault.append_review("mac-2", "3\tc-1\t4\n").unwrap();
    assert_eq!(vault.review_logs().unwrap(), [("mac-1".to_owned(), 16), ("mac-2".to_owned(), 8)]);
    assert_eq!(vault.read_review_log("mac-1", 0).unwrap(), "1\tc-1\t3\n2\tc-2\t1\n");
    assert_eq!(vault.read_review_log("mac-1", 8).unwrap(), "2\tc-2\t1\n", "from where it was read to");
    assert!(dir.path().join(".knotes/reviews/mac-1.tsv").is_file());
    assert!(vault.append_review("../out", "x").is_err() && vault.append_review("", "x").is_err(), "a name, not a path");
}
