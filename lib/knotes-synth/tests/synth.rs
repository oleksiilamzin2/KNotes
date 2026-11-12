use std::collections::HashSet;

use knotes_core::{LinkKind, NoteParser};
use knotes_md::MarkdownParser;
use knotes_synth::{SyntheticVault, VaultSpec};

fn vault(notes: usize, seed: u64) -> SyntheticVault {
    SyntheticVault::plan(VaultSpec::new(notes, seed))
}

#[test]
fn same_seed_gives_same_vault_and_other_seed_differs() {
    let (a, b, c) = (vault(200, 7), vault(200, 7), vault(200, 8));
    for i in [0, 1, 57, 199] {
        assert_eq!(a.path(i), b.path(i));
        assert_eq!(a.note(i), b.note(i));
    }
    assert!((0..200).any(|i| a.note(i) != c.note(i)));
}

#[test]
fn paths_are_unique_markdown_files_in_folders() {
    let v = vault(2000, 1);
    assert_eq!(v.len(), 2000);
    let paths: HashSet<String> = (0..v.len()).map(|i| v.path(i)).collect();
    assert_eq!(paths.len(), 2000);
    assert!(paths.iter().all(|p| p.ends_with(".md") && p.contains('/')));
    let folders: HashSet<&str> = paths.iter().map(|p| p.rsplit_once('/').unwrap().0).collect();
    assert!(folders.len() >= 4, "{} folders", folders.len());
}

#[test]
fn sizes_are_continuous_from_one_line_to_many_pages() {
    let v = vault(3000, 2);
    let mut sizes: Vec<usize> = (0..v.len()).map(|i| v.note(i).len()).collect();
    sizes.sort_unstable();
    let median = sizes[sizes.len() / 2];
    let mean = sizes.iter().sum::<usize>() / sizes.len();
    assert!((1500..=4000).contains(&median), "median {median}");
    assert!((3500..=7500).contains(&mean), "mean {mean}");
    for (lo, hi) in [(0, 300), (300, 1_000), (1_000, 10_000), (10_000, 30_000), (30_000, usize::MAX)] {
        assert!(sizes.iter().any(|&s| s >= lo && s < hi), "no notes in {lo}..{hi}");
    }
    assert!(*sizes.last().unwrap() <= VaultSpec::new(1, 0).max_bytes + 4096);
}

#[test]
fn both_languages_are_present() {
    let v = vault(500, 3);
    let cyrillic = (0..v.len())
        .filter(|&i| {
            let n = v.note(i);
            n.chars().filter(|c| ('а'..='я').contains(c)).count() > n.chars().filter(char::is_ascii_lowercase).count()
        })
        .count();
    assert!((100..=400).contains(&cyrillic), "{cyrillic} Ukrainian notes of 500");
}

#[test]
fn most_wikilinks_resolve_some_are_broken_and_tags_are_common() {
    let v = vault(1000, 4);
    let titles: HashSet<&str> = (0..v.len()).map(|i| v.title(i)).collect();
    let parser = MarkdownParser::new();
    let (mut resolved, mut broken, mut tagged) = (0, 0, 0);
    for i in 0..v.len() {
        let note = parser.parse(&v.note(i));
        for l in note.links.iter().filter(|l| l.kind == LinkKind::Wiki && !l.embed) {
            if titles.contains(l.target.as_str()) { resolved += 1 } else { broken += 1 }
        }
        let fm_tags = note.frontmatter.as_ref().is_some_and(|f| !f.tags.is_empty());
        if !note.tags.is_empty() || fm_tags {
            tagged += 1;
        }
    }
    let total = resolved + broken;
    assert!(total > 3000, "{total} wikilinks");
    assert!(broken > 0 && broken * 100 < total * 10, "{broken} broken of {total}");
    assert!(tagged > 200, "{tagged} tagged notes");
}

#[test]
fn writes_the_planned_files() {
    let v = vault(40, 5);
    let dir = tempfile::tempdir().unwrap();
    let bytes = v.write_to(dir.path()).unwrap();
    let mut total = 0;
    for i in 0..v.len() {
        let on_disk = std::fs::read_to_string(dir.path().join(v.path(i))).unwrap();
        assert_eq!(on_disk, v.note(i));
        total += on_disk.len() as u64;
    }
    assert_eq!(bytes, total);
}
