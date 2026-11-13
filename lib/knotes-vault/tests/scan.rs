use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use knotes_core::{FileKind, VaultScanner};
use knotes_vault::FsVault;

fn touch(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn scan(root: &Path) -> Vec<(String, FileKind)> {
    FsVault::new(root).scan().unwrap().into_iter().map(|f| (f.path, f.kind)).collect()
}

#[test]
fn finds_files_with_kinds_and_relative_slash_paths_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    touch(r, "b.md", "x");
    touch(r, "Folder/Sub/a.md", "x");
    touch(r, "Folder/board.canvas", "{}");
    touch(r, "att/Paper.PDF", "%PDF");
    touch(r, "att/pic.png", "png");
    touch(r, "att/data.csv", "1,2");
    assert_eq!(
        scan(r),
        [
            ("Folder/Sub/a.md".to_owned(), FileKind::Markdown),
            ("Folder/board.canvas".to_owned(), FileKind::Canvas),
            ("att/Paper.PDF".to_owned(), FileKind::Pdf),
            ("att/data.csv".to_owned(), FileKind::Other),
            ("att/pic.png".to_owned(), FileKind::Image),
            ("b.md".to_owned(), FileKind::Markdown),
        ]
    );
}

#[test]
fn skips_hidden_files_and_folders() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    touch(r, "note.md", "x");
    touch(r, ".obsidian/app.json", "{}");
    touch(r, ".trash/old.md", "x");
    touch(r, ".git/HEAD", "ref");
    touch(r, ".DS_Store", "");
    touch(r, "Folder/.hidden.md", "x");
    assert_eq!(scan(r), [("note.md".to_owned(), FileKind::Markdown)]);
}

#[test]
fn reports_size_and_mtime() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "n.md", "hello");
    let f = &FsVault::new(dir.path()).scan().unwrap()[0];
    let meta = fs::metadata(dir.path().join("n.md")).unwrap();
    let mtime = meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_nanos() as i64;
    assert_eq!((f.size, f.mtime_ns), (5, mtime));
}

#[test]
fn ukrainian_names() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "Проєкти/Радар ЧМ.md", "x");
    assert_eq!(scan(dir.path())[0].0, "Проєкти/Радар ЧМ.md");
}

#[test]
fn symlinks_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    touch(r, "Folder/n.md", "x");
    std::os::unix::fs::symlink(r.join("Folder"), r.join("Folder/loop")).unwrap();
    std::os::unix::fs::symlink(r.join("Folder/n.md"), r.join("link.md")).unwrap();
    assert_eq!(scan(r), [("Folder/n.md".to_owned(), FileKind::Markdown)]);
}

#[test]
fn takes_in_the_attachments_in_the_knotes_folder_only() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    touch(r, "note.md", "x");
    touch(r, ".knotes/attachments/Pasted image 20261113114500.png", "png");
    touch(r, ".knotes/attachments/.DS_Store", "");
    touch(r, ".knotes/cache/x.bin", "x");
    touch(r, "Folder/.knotes/attachments/no.png", "png");
    assert_eq!(
        scan(r),
        [(".knotes/attachments/Pasted image 20261113114500.png".to_owned(), FileKind::Image), ("note.md".to_owned(), FileKind::Markdown)]
    );
}
