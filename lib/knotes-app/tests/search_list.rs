use knotes_app::{ContentMatch, ListFilter, ListItem, ListSettings, found_snippet, search_list};
use knotes_core::NoteRow;

#[test]
fn the_snippet_starts_near_the_first_match_without_markup() {
    let text = "## Вступ\n\nПерший абзац про щось зовсім інше, довгий і нудний.\n\nТут **ми** говоримо про радіолокацію і сонари.";
    let s = found_snippet(text, "радіолокація", 50);
    assert!(s.contains("радіолокацію"), "{s}");
    assert!(s.starts_with('…') && s.chars().count() <= 52, "{s}");
    assert!(!s.contains("**") && !s.contains('\n'), "{s}");
}

#[test]
fn word_forms_and_case_match_at_word_starts() {
    let s = found_snippet("Вона пише МОВОЮ оригіналу, а не перекладом", "мова", 80);
    assert!(s.starts_with("Вона пише МОВОЮ"), "{s}");
    assert_eq!(found_snippet("нічого спільного", "зовсім", 80), "нічого спільного", "no match: the start of the text");
}

fn row(path: &str) -> NoteRow {
    NoteRow { path: path.into(), mtime_ns: 1, snippet: String::new() }
}

#[test]
fn title_matches_come_first_then_text_matches_within_the_list() {
    let rows = [row("Projects/c.md"), row("Projects/d.md"), row("a.md")];
    let content = [
        ContentMatch { path: "Projects/c.md".into(), snippet: "…c fragment".into() },
        ContentMatch { path: "Projects/d.md".into(), snippet: "…d fragment".into() },
        ContentMatch { path: "a.md".into(), snippet: "outside".into() },
    ];
    let titles = ["a.md".to_owned(), "Projects/d.md".to_owned()];
    let items = search_list(&rows, &ListFilter::Folder("Projects"), &ListSettings::default(), &titles, &content);
    assert_eq!(items, [ListItem::Found(1, "…d fragment".into()), ListItem::Found(0, "…c fragment".into())]);
    let only_title = search_list(&rows, &ListFilter::All, &ListSettings::default(), &["a.md".to_owned()], &[]);
    assert_eq!(only_title, [ListItem::Note(2)], "a title match without a text match keeps the usual snippet");
}

#[test]
fn found_words_are_the_whole_words_that_start_like_a_query_word() {
    let text = "Радар і радари, **радарний** модуль; не арадар.";
    let words: Vec<&str> = knotes_app::found_words(text, "радар").into_iter().map(|r| &text[r]).collect();
    assert_eq!(words, ["Радар", "радари", "радарний"]);
    let words: Vec<&str> = knotes_app::found_words("fast cars and Cats", "cat fast").into_iter().map(|r| &"fast cars and Cats"[r]).collect();
    assert_eq!(words, ["fast", "Cats"]);
    assert!(knotes_app::found_words("text", "").is_empty());
}

#[test]
fn a_pdf_found_by_its_text_is_an_entry_of_its_own() {
    let rows = vec![knotes_core::NoteRow { path: "a.md".into(), mtime_ns: 1, snippet: String::new() }, knotes_core::NoteRow { path: "doc.pdf".into(), mtime_ns: 2, snippet: String::new() }];
    let content = [knotes_app::ContentMatch { path: "doc.pdf".into(), snippet: "Page 2".into() }];
    let settings = knotes_app::ListSettings { sort: knotes_app::SortOrder::Title, group_by_date: false, include_subfolders: true };
    let items = knotes_app::search_list(&rows, &knotes_app::ListFilter::All, &settings, &[], &content);
    assert_eq!(items, [knotes_app::ListItem::Found(1, "Page 2".into())]);
}
