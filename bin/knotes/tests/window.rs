//! Launches the real app in probe mode: it builds the window, prints its structure and quits.

use std::process::Command;
use std::time::{Duration, Instant};

fn probe() -> String {
    probe_with(&[], None)
}

/// Runs the app in probe mode with a throwaway HOME, so no real settings or index are touched.
fn probe_with(args: &[&std::path::Path], home: Option<&std::path::Path>) -> String {
    probe_env(args, home, &[])
}

fn probe_env(args: &[&std::path::Path], home: Option<&std::path::Path>, env: &[(&str, &str)]) -> String {
    let scratch_home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes"))
        .args(args)
        .envs(env.iter().copied())
        .env("KNOTES_PROBE", "1")
        .env("HOME", home.unwrap_or(scratch_home.path()))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("app starts");
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut out = String::new();
            std::io::Read::read_to_string(&mut child.stdout.take().unwrap(), &mut out).unwrap();
            assert!(status.success(), "app failed: {out}");
            return out;
        }
        if start.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("app did not quit in probe mode");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn window_has_four_columns_with_native_roles() {
    let out = probe();
    // The list and the note scroll under the toolbar's glass (an opaque titlebar keeps it),
    // and their scroll pockets draw no separator line (checked on screen captures; offscreen
    // rendering shows neither).
    assert!(out.contains("titlebar: opaque"), "{out}");
    assert!(out.contains("column-pockets: 2, separators on: 0"), "{out}");
    assert!(out.contains("columns: sidebar, content-list, content, inspector"), "{out}");
    assert!(out.contains("title: Knotes"), "{out}");
}

#[test]
fn window_opens_at_a_usable_size() {
    let out = probe();
    let size = out.lines().find_map(|l| l.strip_prefix("size: ")).expect("size line");
    let (w, h) = size.split_once('x').unwrap();
    let (w, h): (f64, f64) = (w.parse().unwrap(), h.parse().unwrap());
    assert!(w >= 1000.0 && h >= 600.0, "{size}");
}

#[test]
fn sidebar_shows_all_notes_folders_and_tags_of_the_vault() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Projects/Radar")).unwrap();
    std::fs::create_dir_all(vault.path().join("Inbox")).unwrap();
    std::fs::write(vault.path().join("Projects/Radar/a.md"), "#radar one").unwrap();
    std::fs::write(vault.path().join("Projects/b.md"), "two").unwrap();
    std::fs::write(vault.path().join("Inbox/c.md"), "three #todo").unwrap();
    let out = probe_with(&[vault.path()], None);
    let sidebar = out.lines().find_map(|l| l.strip_prefix("sidebar: ")).unwrap_or_else(|| panic!("{out}"));
    // A folder counts what its list shows: its own notes (Radar's are in its own row).
    assert_eq!(sidebar, "Усі нотатки (3) | Вхідні (0) | Без тегів (1) | · | Inbox (1) | Projects (1)", "folders unless switched, no heading");
    let size = out.lines().find_map(|l| l.strip_prefix("size: ")).expect("size line");
    let width: f64 = size.split_once('x').unwrap().0.parse().unwrap();
    assert!(width >= 1000.0, "window shrank after data arrived: {size}");
}

fn write_aged(path: &std::path::Path, text: &str, age: Duration) {
    std::fs::write(path, text).unwrap();
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(std::time::SystemTime::now() - age).unwrap();
}

#[test]
fn note_list_shows_date_groups_titles_times_and_snippets() {
    let vault = tempfile::tempdir().unwrap();
    let day = Duration::from_secs(86_400);
    write_aged(&vault.path().join("A.md"), "# A\n\nbody a", Duration::from_secs(60));
    write_aged(&vault.path().join("B.md"), "body b", day + Duration::from_secs(3600));
    write_aged(&vault.path().join("C.md"), "body c", day * 40);
    let out = probe_with(&[vault.path()], None);
    assert!(out.contains("title: Усі нотатки"), "{out}");
    assert!(out.contains("subtitle: 3 нотатки"), "{out}");
    let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap_or_else(|| panic!("{out}"));
    let rows: Vec<&str> = list.split(" | ").collect();
    assert_eq!(rows.len(), 6, "{list}");
    assert_eq!((rows[0], rows[2]), ("[Сьогодні]", "[Вчора]"), "{list}");
    assert!(rows[4].starts_with('[') && rows[4].ends_with(']'), "{list}");
    for (row, (title, snippet)) in [rows[1], rows[3], rows[5]].iter().zip([("A", "body a"), ("B", "body b"), ("C", "body c")]) {
        let parts: Vec<&str> = row.split(" ~ ").collect();
        assert_eq!(parts.len(), 3, "{row}");
        assert_eq!((parts[0], parts[2]), (title, snippet), "{row}");
        assert!(!parts[1].is_empty(), "{row}");
    }
}

#[test]
fn toolbar_has_v1_buttons_and_columns_have_room() {
    let out = probe();
    let toolbar = out.lines().find_map(|l| l.strip_prefix("toolbar: ")).unwrap_or_else(|| panic!("{out}"));
    assert_eq!(
        toolbar,
        "toggle-sidebar, sidebar-mode(2), sidebar-separator, search, list-options, list-separator, new-note, history(2), note-title, flexible-space, inspector-separator, chat"
    );
    let widths = out.lines().find_map(|l| l.strip_prefix("widths: ")).unwrap_or_else(|| panic!("{out}"));
    let w: Vec<f64> = widths.split(' ').map(|x| x.parse().unwrap()).collect();
    assert!(w[0] >= 240.0 && w[1] >= 300.0 && w[3] >= 280.0 && w[2] >= 400.0, "{widths}");
}

fn list_titles(out: &str) -> Vec<String> {
    let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap_or_else(|| panic!("{out}"));
    list.split(" | ").filter(|r| !r.starts_with('[')).map(|r| r.split(" ~ ").next().unwrap().to_owned()).collect()
}

#[test]
fn selecting_a_folder_or_tag_filters_the_list() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Projects/Sub")).unwrap();
    std::fs::create_dir_all(vault.path().join("Inbox")).unwrap();
    write_aged(&vault.path().join("Projects/a.md"), "#radar a", Duration::from_secs(10));
    write_aged(&vault.path().join("Projects/Sub/b.md"), "b", Duration::from_secs(20));
    write_aged(&vault.path().join("Inbox/c.md"), "#radar c", Duration::from_secs(30));

    // A folder lists its own notes; its subfolders' only when asked.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "f:Projects")]);
    assert!(out.contains("title: Projects") && out.contains("subtitle: 1 нотатка"), "{out}");
    assert_eq!(list_titles(&out), ["a"]);
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_ACTION", "subfolders=on")]);
    assert_eq!(list_titles(&out), ["a", "b"]);

    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "t:radar")]);
    assert!(out.contains("title: #radar") && out.contains("subtitle: 2 нотатки"), "{out}");
    assert_eq!(list_titles(&out), ["a", "c"]);
}

fn projects_vault() -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Projects")).unwrap();
    write_aged(&vault.path().join("Projects/b note.md"), "b", Duration::from_secs(10));
    write_aged(&vault.path().join("Projects/a note.md"), "a", Duration::from_secs(20));
    vault
}

#[test]
fn list_options_menu_shows_the_folder_settings() {
    let vault = projects_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "f:Projects")]);
    let menu = out.lines().find_map(|l| l.strip_prefix("list-menu: ")).unwrap_or_else(|| panic!("{out}"));
    assert_eq!(menu, "Сортувати за > [✓ Датою зміни, Назвою] | ✓ Групувати за датою | Нотатки з підтек");
}

#[test]
fn choosing_a_sort_order_reorders_and_saves_only_that_field() {
    let vault = projects_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_ACTION", "sort=title")]);
    assert_eq!(list_titles(&out), ["a note", "b note"]);
    assert!(out.contains("list-menu: Сортувати за > [Датою зміни, ✓ Назвою]"), "{out}");
    let saved = std::fs::read_to_string(vault.path().join("Projects/.knotes.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(json, serde_json::json!({"list": {"sort": "title"}}));
}

#[test]
fn clicking_notes_opens_them_and_back_returns() {
    let vault = tempfile::tempdir().unwrap();
    write_aged(&vault.path().join("a.md"), "# A\n\nbody a", Duration::from_secs(10));
    write_aged(&vault.path().join("b.md"), "body b", Duration::from_secs(20));
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "a.md,b.md"), ("KNOTES_PROBE_ACTION", "back")]);
    assert!(out.contains("note: a.md | # A⏎⏎body a"), "{out}");
    assert!(out.contains("nav: back=off forward=on"), "{out}");
    assert!(out.contains("selected-row: a"), "{out}");
}

#[test]
fn chat_column_has_title_and_question_field() {
    let out = probe();
    assert!(out.contains("chat: Чат | Запитайте свої нотатки…"), "{out}");
}

#[test]
fn main_menu_has_the_standard_menus() {
    let out = probe();
    assert!(out.contains("menu: Knotes | Файл | Редагування | Вигляд | Вікно"), "{out}");
    assert!(out.contains("file-menu: Нова нотатка ⌘N | Відкрити базу… ⌘O | Зберегти ⌘S | Закрити вікно ⌘W"), "{out}");
}

#[test]
fn the_last_vault_reopens_without_arguments() {
    let (vault, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(vault.path().join("only.md"), "x").unwrap();
    probe_with(&[vault.path()], Some(home.path()));
    let out = probe_with(&[], Some(home.path()));
    assert!(out.contains("sidebar: Усі нотатки (1)"), "{out}");
}

#[test]
fn closing_the_last_window_quits_the_app() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "x").unwrap();
    // probe_env waits for the process to exit on its own; in "close" mode the probe only
    // closes the window instead of terminating.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "close")]);
    assert!(out.contains("closing window"), "{out}");
}

#[test]
fn scrolling_the_note_list_fits_a_120_hz_frame() {
    let vault = tempfile::tempdir().unwrap();
    for i in 0..400 {
        std::fs::write(vault.path().join(format!("note {i}.md")), format!("line of note {i} with some words\n")).unwrap();
    }
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "scroll-bench")]);
    let line = out.lines().find(|l| l.starts_with("scroll-bench:")).unwrap_or_else(|| panic!("{out}"));
    let p95: f64 = line.split("p95=").nth(1).unwrap().split("ms").next().unwrap().parse().unwrap();
    assert!(p95 < 8.33, "{line}");
}

fn editor_runs(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("editor: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn live_preview_hides_markup_outside_the_cursor_line() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "## Title\n\nsome **bold** text\n\nlast line").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md")]);
    assert!(out.contains("textkit: 2"), "{out}");
    assert_eq!(
        editor_runs(&out),
        "[markup]## |[heading2]Title|[plain]⏎⏎some |[hidden]**|[strong]bold|[hidden]**|[plain] text⏎⏎last line"
    );
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert_eq!(
        editor_runs(&out),
        "[hidden]## |[heading2]Title|[plain]⏎⏎some |[hidden]**|[strong]bold|[hidden]**|[plain] text⏎⏎last line"
    );
}

fn text_layout(out: &str) -> (f64, f64, f64) {
    let line = out.lines().find_map(|l| l.strip_prefix("text-layout: ")).unwrap_or_else(|| panic!("{out}"));
    let v: Vec<f64> = line.split(' ').map(|kv| kv.split('=').nth(1).unwrap().parse().unwrap()).collect();
    (v[0], v[1], v[2])
}

#[test]
fn note_text_is_at_most_700_points_wide_and_centered() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "text").unwrap();
    // Default window: the note column is narrower than 700 + margins.
    let (column, width, left) = text_layout(&probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md")]));
    assert!(column < 764.0, "column {column}");
    assert!((width - (column - 64.0)).abs() < 0.2 && left == 32.0, "{column} {width} {left}");
    // Wide window: exactly 700 points, centered.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_WIDTH", "2400")]);
    let (column, width, left) = text_layout(&out);
    assert!(column > 900.0, "column {column}");
    assert!((width - 700.0).abs() < 0.2, "width {width}");
    assert!((left - (column - 700.0) / 2.0).abs() < 0.2, "left {left} of column {column}");
}

fn read(vault: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(vault.join(name)).unwrap()
}

#[test]
fn typing_is_saved_two_seconds_later_without_moving_the_cursor() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    let out = probe_env(
        &[vault.path()],
        None,
        &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_TYPE", " world"), ("KNOTES_PROBE_DELAY_MS", "3500")],
    );
    assert_eq!(read(vault.path(), "n.md"), "hello world");
    assert!(out.contains("document: clean"), "{out}");
    assert!(out.contains("cursor: 11"), "own save and the snapshot after it must not reload: {out}");
}

#[test]
fn nothing_is_saved_before_two_seconds_but_quitting_saves() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_TYPE", "!")]);
    assert!(out.contains("document: dirty"), "{out}");
    assert!(out.contains("disk: \"hello\""), "{out}");
    assert_eq!(read(vault.path(), "n.md"), "hello!", "quitting must write unsaved edits");
}

#[test]
fn cmd_s_saves_at_once() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_TYPE", "!"), ("KNOTES_PROBE_ACTION", "save")]);
    assert!(out.contains("document: clean"), "{out}");
    assert!(out.contains("disk: \"hello!\""), "{out}");
}

#[test]
fn opening_another_note_saves_the_previous_one() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    std::fs::write(vault.path().join("b.md"), "b").unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "a.md,b.md"), ("KNOTES_PROBE_TYPE", "+")]);
    assert_eq!(read(vault.path(), "a.md"), "a+");
}

/// The editor line after typing `typed` at the end of `start` equals the one after opening the
/// final text with the cursor at the end: restyling only the edited region loses nothing.
fn assert_typing_restyles_like_opening(start: &str, typed: &str) {
    let end = start.encode_utf16().count();
    assert_replacing_restyles_like_opening(start, end, 0, typed);
}

/// Same for replacing `removed` UTF-16 units at `at` with `typed` (ASCII texts).
fn assert_replacing_restyles_like_opening(start: &str, at: usize, removed: usize, typed: &str) {
    let editor = |vault: &std::path::Path, env: &[(&str, &str)]| {
        let out = probe_env(&[vault], None, env);
        out.lines().find(|l| l.starts_with("editor: ")).unwrap_or_default().to_owned()
    };
    let typing = tempfile::tempdir().unwrap();
    std::fs::write(typing.path().join("n.md"), start).unwrap();
    let range = format!("{at},{removed}");
    let after_typing = editor(typing.path(), &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_TYPE", typed), ("KNOTES_PROBE_RANGE", &range)]);
    let opening = tempfile::tempdir().unwrap();
    let text = format!("{}{typed}{}", &start[..at], &start[at + removed..]);
    std::fs::write(opening.path().join("n.md"), text).unwrap();
    let cursor = (at + typed.len()).to_string();
    let after_opening = editor(opening.path(), &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", &cursor)]);
    assert_eq!(after_typing, after_opening, "replaced {at},{removed} with {typed:?}");
}

#[test]
fn deleting_a_setext_underline_restyles_the_line_above() {
    assert_replacing_restyles_like_opening("Title\n===\n\nbody **b**", 5, 4, "");
}

#[test]
fn opening_a_fence_above_text_restyles_everything_below() {
    assert_replacing_restyles_like_opening("intro\n\n**a** and _b_\n\n# H", 7, 0, "```\n");
}

#[test]
fn closing_a_fence_restyles_the_text_after_it() {
    assert_replacing_restyles_like_opening("```\ncode\n\n**a**\n\n# H", 9, 0, "```\n");
}

#[test]
fn typing_inline_markup_restyles_like_opening() {
    assert_typing_restyles_like_opening("# Title\n\nhello", " **b** _i_ `c` [[Link]] #tag");
}

#[test]
fn typing_a_heading_restyles_like_opening() {
    assert_typing_restyles_like_opening("intro **x**\n\ntext", "\n## Heading");
}

#[test]
fn typing_a_code_block_at_the_end_restyles_like_opening() {
    assert_typing_restyles_like_opening("# Title\n\n**a**\n\nb", "\n```\nlet x = 1;");
}

#[test]
fn an_outside_change_to_a_clean_note_reloads_it() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    let out = probe_env(
        &[vault.path()],
        None,
        &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_WRITE", "changed outside"), ("KNOTES_PROBE_DELAY_MS", "1500")],
    );
    assert!(out.contains("note: n.md | changed outside"), "{out}");
    assert!(out.contains("document: clean"), "{out}");
}

#[test]
fn an_outside_change_to_an_edited_note_keeps_both_versions() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    let out = probe_env(
        &[vault.path()],
        None,
        &[
            ("KNOTES_PROBE_OPEN", "n.md"),
            ("KNOTES_PROBE_TYPE", "!"),
            ("KNOTES_PROBE_WRITE", "changed outside"),
            ("KNOTES_PROBE_DELAY_MS", "1500"),
        ],
    );
    assert_eq!(read(vault.path(), "n.md"), "hello!", "the editor's text keeps the name: {out}");
    let copies: Vec<String> = std::fs::read_dir(vault.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("n (конфлікт ") && n.ends_with(").md"))
        .collect();
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(read(vault.path(), &copies[0]), "changed outside");
    assert!(out.contains(&format!("conflict-notice: {}", copies[0])), "the user is told where the other version went: {out}");
    assert!(out.contains("document: clean"), "{out}");
}

/// A vault with the given notes, probed with `env`.
fn probe_notes(notes: &[(&str, &str)], env: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in notes {
        let file = vault.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let out = probe_env(&[vault.path()], None, env);
    (vault, out)
}

#[test]
fn clicking_a_link_outside_the_cursor_line_opens_its_note() {
    let (_vault, out) = probe_notes(
        &[("a.md", "[[b]]\n\nsecond line"), ("b.md", "# B\n\ntarget")],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "2")],
    );
    assert!(out.contains("note: b.md | # B⏎⏎target"), "{out}");
    assert!(out.contains("nav: back=on"), "{out}");
}

#[test]
fn a_plain_click_in_the_cursor_line_places_the_cursor_and_cmd_click_opens() {
    let notes = [("a.md", "see [[b]] here"), ("b.md", "target")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CLICK", "6")]);
    assert!(out.contains("note: a.md"), "{out}");
    assert!(out.contains("cursor: 6") || out.contains("cursor: 7"), "{out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CLICK", "6:cmd")]);
    assert!(out.contains("note: b.md | target"), "{out}");
}

#[test]
fn clicking_a_heading_link_moves_the_cursor_to_the_heading() {
    let b = format!("# B\n\n{}## Second\n\ntext", "line\n\n".repeat(80));
    let (_vault, out) = probe_notes(
        &[("a.md", "[[b#second]]\n\nx"), ("b.md", &b)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "3")],
    );
    assert!(out.contains("note: b.md"), "{out}");
    assert!(out.contains(&format!("cursor: {}", b.find("## Second").unwrap())), "{out}");
}

#[test]
fn clicking_a_link_to_a_missing_note_creates_it_in_the_new_notes_folder() {
    let (vault, out) = probe_notes(
        &[("a.md", "[[Нова ідея]]\n\nx")],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "3")],
    );
    assert!(vault.path().join("Вхідні/Нова ідея.md").is_file(), "{out}");
    assert!(out.contains("note: Вхідні/Нова ідея.md"), "{out}");
    let (vault, _) = probe_notes(
        &[("a.md", "[[Ідея]]\n\nx"), (".knotes.json", r#"{"newNotes": {"folder": "Inbox"}}"#)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "3")],
    );
    assert!(vault.path().join("Inbox/Ідея.md").is_file());
}

#[test]
fn links_to_missing_notes_are_dimmed() {
    let (_vault, out) = probe_notes(&[("a.md", "[[b]] [[zzz]]\n\nx"), ("b.md", "b")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[wikilink]b|") && runs.contains("[missing]zzz|"), "{runs}");
}

#[test]
fn clicking_an_external_link_opens_it_outside() {
    let (_vault, out) = probe_notes(
        &[("a.md", "[site](https://example.com)\n\nx")],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "2")],
    );
    assert!(out.contains("open-url: https://example.com"), "{out}");
}

#[test]
fn opening_a_shorter_note_after_a_longer_one_with_markup_works() {
    let (_vault, out) = probe_notes(
        &[("a.md", "see **bold** and [[b]] here, a longer note"), ("b.md", "short")],
        &[("KNOTES_PROBE_OPEN", "a.md,b.md")],
    );
    assert!(out.contains("note: b.md | short"), "{out}");
}

fn completion(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("completion: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn typing_an_open_wikilink_lists_matching_notes_with_their_folders() {
    let (_vault, out) = probe_notes(
        &[("Projects/Radar.md", "r"), ("Radio.md", "r"), ("a.md", "x")],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", " [[Rad")],
    );
    let list = completion(&out);
    assert!(list.contains("Radar — Projects") && list.contains("Radio"), "{list}");
    assert!(list.starts_with('▸'), "the first row is chosen: {list}");
}

#[test]
fn return_inserts_the_chosen_note_as_the_shortest_link_and_closes_it() {
    let notes = [("Note.md", "n"), ("A/Note.md", "n"), ("a.md", "")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[Note"), ("KNOTES_PROBE_KEYS", "return")]);
    assert!(out.contains("note: a.md | [[Note]]"), "{out}");
    assert!(out.contains("cursor: 8"), "{out}");
    assert_eq!(completion(&out), "none");
    let (_vault, out) =
        probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[Note"), ("KNOTES_PROBE_KEYS", "down,return")]);
    assert!(out.contains("note: a.md | [[A/Note]]"), "the bare name would open Note.md: {out}");
}

#[test]
fn after_a_hash_the_list_offers_the_target_headings() {
    let (_vault, out) = probe_notes(
        &[("b.md", "# B\n\n## Second part\n\n## Other"), ("a.md", "")],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[b#sec"), ("KNOTES_PROBE_KEYS", "tab")],
    );
    assert!(out.contains("note: a.md | [[b#Second part]]"), "{out}");
}

#[test]
fn escape_closes_the_list_and_leaves_the_text() {
    let (_vault, out) = probe_notes(
        &[("Note.md", "n"), ("a.md", "")],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[No"), ("KNOTES_PROBE_KEYS", "escape")],
    );
    assert_eq!(completion(&out), "none");
    assert!(out.contains("note: a.md | [[No"), "{out}");
}

fn footprint_mb(out: &str) -> f64 {
    out.lines().find_map(|l| l.strip_prefix("memory: footprint ")?.strip_suffix(" MB")?.parse().ok()).unwrap_or_else(|| panic!("{out}"))
}

/// A cold index must not leave the app much bigger than opening the same vault again: the
/// SQLite page cache is freed and large blocks bypass malloc's cache (ADR 0006, 0014). At 8k
/// notes the debug app measured +90 MB with malloc's large cache and +56 MB with a 64 MB
/// SQLite write cache; now about +33 MB (fragmented small blocks) is left.
#[test]
fn a_cold_index_leaves_no_more_memory_behind_than_a_warm_start() {
    let vault = tempfile::tempdir().unwrap();
    knotes_synth::SyntheticVault::plan(knotes_synth::VaultSpec::new(8000, 3)).write_to(vault.path()).unwrap();
    let home = tempfile::tempdir().unwrap();
    let env = [("KNOTES_PROBE_DELAY_MS", "7000")];
    let cold = footprint_mb(&probe_env(&[vault.path()], Some(home.path()), &env));
    let warm = footprint_mb(&probe_env(&[vault.path()], Some(home.path()), &env));
    assert!(cold - warm < 45.0, "cold {cold} MB, warm {warm} MB");
}

#[test]
fn cmd_n_creates_an_untitled_note_in_the_selected_folder_and_focuses_it() {
    let (vault, out) = probe_notes(&[("Projects/a.md", "a")], &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_ACTION", "new")]);
    assert!(vault.path().join("Projects/Без назви.md").is_file(), "{out}");
    assert!(out.contains("note: Projects/Без назви.md"), "{out}");
    assert!(out.contains("focus: editor"), "{out}");
}

#[test]
fn without_a_selected_folder_new_notes_go_to_the_new_notes_folder_numbered() {
    let (vault, out) = probe_notes(&[("a.md", "a"), ("Вхідні/Без назви.md", "")], &[("KNOTES_PROBE_ACTION", "new")]);
    assert!(vault.path().join("Вхідні/Без назви 1.md").is_file(), "{out}");
    assert!(out.contains("note: Вхідні/Без назви 1.md"), "{out}");
}

#[test]
fn return_after_the_first_line_names_the_note_after_it() {
    let (vault, out) = probe_notes(
        &[("a.md", "a")],
        &[("KNOTES_PROBE_ACTION", "new"), ("KNOTES_PROBE_TYPE", "# Список покупок\nмолоко"), ("KNOTES_PROBE_DELAY_MS", "3500")],
    );
    assert_eq!(read(vault.path(), "Вхідні/Список покупок.md"), "# Список покупок\nмолоко", "{out}");
    assert!(!vault.path().join("Вхідні/Без назви.md").exists());
    assert!(out.contains("note: Вхідні/Список покупок.md"), "{out}");
    assert!(out.contains("selected-row: Список покупок"), "{out}");
}

#[test]
fn an_unfinished_first_line_names_the_note_when_leaving_it() {
    let (vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "new"), ("KNOTES_PROBE_TYPE", "Ідея")]);
    assert!(out.contains("note: Вхідні/Без назви.md"), "not named mid-line: {out}");
    assert_eq!(read(vault.path(), "Вхідні/Ідея.md"), "Ідея", "quitting leaves the note");
    assert!(!vault.path().join("Вхідні/Без назви.md").exists());
}

#[test]
fn the_sidebar_switch_shows_tags_instead_of_folders_and_is_remembered() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Inbox")).unwrap();
    std::fs::write(vault.path().join("Inbox/c.md"), "three #todo").unwrap();
    std::fs::write(vault.path().join("a.md"), "#radar one").unwrap();
    let home = tempfile::tempdir().unwrap();
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_ACTION", "sidebar=tags")]);
    assert!(out.contains("sidebar: Усі нотатки (2) | Вхідні (0) | Без тегів (0) | · | radar (1) | todo (1)"), "{out}");
    assert!(out.contains("sidebar-mode: tags"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[]);
    assert!(out.contains("sidebar: Усі нотатки (2) | Вхідні (0) | Без тегів (0) | · | radar (1) | todo (1)"), "remembered: {out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_ACTION", "sidebar=folders")]);
    assert!(out.contains("sidebar: Усі нотатки (2) | Вхідні (0) | Без тегів (0) | · | Inbox (1)"), "{out}");
}

#[test]
fn the_sidebar_switch_hides_with_the_sidebar() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    let out = probe_env(&[vault.path()], None, &[]);
    assert!(out.contains("sidebar-mode: folders"), "{out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "toggle-sidebar")]);
    assert!(out.contains("sidebar-mode: hidden"), "{out}");
}

/// x of the toolbar's separator line and of the list/note column boundary.
fn separator_and_boundary(out: &str) -> (f64, f64) {
    let line = out.lines().find_map(|l| l.strip_prefix("list-separator: ")).unwrap_or_else(|| panic!("{out}"));
    let (sep, boundary) = line.split_once(" boundary ").unwrap();
    (sep.parse().unwrap(), boundary.parse().unwrap())
}

#[test]
fn the_toolbar_separator_stays_on_the_column_boundary_when_the_sidebar_collapses() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    for action in ["none", "toggle-sidebar"] {
        let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", action)]);
        let (sep, boundary) = separator_and_boundary(&out);
        assert!((sep - boundary).abs() <= 1.5, "{action}: separator at {sep}, boundary at {boundary}");
    }
}

#[test]
fn untagged_lists_the_notes_without_tags() {
    let (_vault, out) = probe_notes(&[("a.md", "#todo a"), ("b.md", "plain b"), ("Вхідні/c.md", "plain c")], &[("KNOTES_PROBE_SELECT", "untagged")]);
    assert!(out.contains("title: Без тегів"), "{out}");
    let mut titles = list_titles(&out);
    titles.sort();
    assert_eq!(titles, ["b", "c"], "{out}");
    assert!(out.contains("sidebar: Усі нотатки (3) | Вхідні (1) | Без тегів (2) | ·"), "{out}");
}

#[test]
fn the_inbox_is_listed_once_and_selectable_before_it_exists() {
    let (_vault, out) = probe_notes(&[("Вхідні/c.md", "c"), ("Projects/p.md", "p")], &[]);
    assert!(out.contains("sidebar: Усі нотатки (2) | Вхідні (1) | Без тегів (2) | · | Projects (1)"), "not twice: {out}");
    let (_vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_SELECT", "f:Вхідні")]);
    assert!(out.contains("title: Вхідні"), "{out}");
    assert!(out.contains("subtitle: 0 нотаток"), "{out}");
}

const SEARCH_NOTES: [(&str, &str); 4] = [
    ("Projects/Radar.md", "about sensors"),
    ("Projects/notes.md", "# Notes\n\nToday we discuss radar modes and filters."),
    ("Projects/other.md", "nothing here"),
    ("outside.md", "radar outside the folder"),
];

#[test]
fn search_looks_at_file_names_by_default_within_the_selected_folder() {
    let (_vault, out) = probe_notes(&SEARCH_NOTES, &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_SEARCH", "radar")]);
    assert_eq!(list_titles(&out), ["Radar"], "{out}");
    assert!(out.contains("search-mode: files"), "{out}");
    assert!(out.contains("subtitle: 1 нотатка"), "{out}");
}

#[test]
fn content_searches_the_text_and_shows_the_found_words() {
    let (_vault, out) = probe_notes(&SEARCH_NOTES, &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_SEARCH", "content: radar")]);
    assert_eq!(list_titles(&out), ["notes"], "{out}");
    let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap();
    assert!(list.contains("radar modes"), "{list}");
    assert!(out.contains("search-mode: content"), "the switch follows the typed operator: {out}");
}

#[test]
fn the_switch_rewrites_the_query_operator() {
    let (_vault, out) = probe_notes(
        &SEARCH_NOTES,
        &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_SEARCH", "radar"), ("KNOTES_PROBE_SEARCH_MODE", "content")],
    );
    assert!(out.contains("search-field: content: radar"), "{out}");
    assert_eq!(list_titles(&out), ["notes"], "{out}");
}

#[test]
fn word_forms_are_found_and_an_empty_search_restores_the_list() {
    let (_vault, out) = probe_notes(
        &[("a.md", "Пишемо українською мовою"), ("b.md", "інше")],
        &[("KNOTES_PROBE_SEARCH", "content: мова")],
    );
    assert_eq!(list_titles(&out), ["a"], "{out}");
    let (_vault, out) = probe_notes(&[("a.md", "a"), ("b.md", "b")], &[("KNOTES_PROBE_SEARCH", "")]);
    assert_eq!(list_titles(&out).len(), 2, "{out}");
}

#[test]
fn the_search_field_scrolls_to_show_what_is_typed() {
    let long = "content: Люцифер Баптист і ще дуже довгий запит, що не влазить у поле пошуку";
    let (_vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_SEARCH", long), ("KNOTES_PROBE_SEARCH_TYPE", "1")]);
    assert!(out.contains(&format!("search-field: {long}")), "{out}");
    assert!(out.contains("search-caret-visible: true"), "the end of the text must stay in view: {out}");
}

fn half_word_notes() -> Vec<(&'static str, String)> {
    let filler = "слово ".repeat(500);
    vec![
        ("razom.md", "Сьогодні Люцифер і Баптист прийшли разом.".to_owned()),
        ("okremo.md", format!("Люцифер тут.\n\n{filler}\n\n# Інший розділ\n\nБаптист там.")),
        ("odyn.md", "Лише Люцифер, без другого.".to_owned()),
    ]
}

#[test]
fn content_finds_halves_of_words_anywhere_in_a_note_and_block_in_one_block() {
    let notes = half_word_notes();
    let refs: Vec<(&str, &str)> = notes.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let (_vault, out) = probe_notes(&refs, &[("KNOTES_PROBE_SEARCH", "content: люц бап")]);
    let mut titles = list_titles(&out);
    titles.sort();
    assert_eq!(titles, ["okremo", "razom"], "{out}");
    let (_vault, out) = probe_notes(&refs, &[("KNOTES_PROBE_SEARCH", "block: люцифер баптист")]);
    assert_eq!(list_titles(&out), ["razom"], "{out}");
}

const FORMULA_NOTE: &str = "Energy $E=mc^2$ here\n\n$$\n\\frac{a+b}{\\sqrt{x^2+y^2}}\n$$\n\nlast line";

/// `formula-image: x y w h kind` lines of the probe (formulas drawn in view).
fn formula_images(out: &str) -> Vec<(f64, f64, f64, f64, String)> {
    out.lines()
        .filter_map(|l| l.strip_prefix("formula-image: "))
        .map(|l| {
            let p: Vec<&str> = l.split(' ').collect();
            (p[0].parse().unwrap(), p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap(), p[4].to_owned())
        })
        .collect()
}

#[test]
fn formulas_show_typeset_outside_the_cursor_with_their_source_hidden() {
    let (_vault, out) = probe_notes(&[("f.md", FORMULA_NOTE)], &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[formula]$E=mc^2$|"), "{runs}");
    assert!(runs.contains("[formula]$$⏎\\frac{a+b}{\\sqrt{x^2+y^2}}⏎$$|"), "{runs}");
    let layers = formula_images(&out);
    assert_eq!(layers.len(), 2, "{out}");
    let layout = out.lines().find_map(|l| l.strip_prefix("text-layout: ")).unwrap();
    let column: f64 = layout.split(' ').find_map(|p| p.strip_prefix("column=")).unwrap().parse().unwrap();
    let (ix, iy, iw, ih, _) = &layers[0];
    assert!(*iy < 60.0 && *iw > 20.0 && *ih > 8.0 && *ix > 40.0, "inline formula on the first line: {layers:?}");
    let (dx, dy, dw, dh, _) = &layers[1];
    assert!(((dx + dw / 2.0) - column / 2.0).abs() < 3.0, "display formula centred in the column {column}: {layers:?}");
    assert!(*dy > iy + ih && *dh > 25.0, "below the first line, two levels tall: {layers:?}");
}

#[test]
fn the_cursor_in_a_display_formula_shows_its_source_and_a_preview_below() {
    let inside = FORMULA_NOTE.find("frac").unwrap().to_string();
    let (_vault, out) = probe_notes(&[("f.md", FORMULA_NOTE)], &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", &inside)]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[math]$$⏎\\frac"), "the source shows: {runs}");
    let layers = formula_images(&out);
    assert!(layers.iter().any(|l| l.4 == "preview"), "{layers:?}");
    assert!(layers.iter().any(|l| l.4 == "typeset"), "the inline one stays typeset: {layers:?}");
}

#[test]
fn invalid_latex_shows_its_source_as_an_error() {
    let (_vault, out) = probe_notes(&[("f.md", "bad $\\nosuchcommand x$ here\n\nend")], &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert!(editor_runs(&out).contains("[math-error]$\\nosuchcommand x$"), "{out}");
    assert!(formula_images(&out).is_empty(), "{out}");
}

#[test]
fn formulas_that_scroll_into_view_get_their_images() {
    let note: String = (0..300).map(|i| format!("Рядок {i} з формулою $x_{{{i}}}^2$ у тексті.\n\n")).collect();
    let (_vault, out) = probe_notes(&[("long.md", &note)], &[("KNOTES_PROBE_OPEN", "long.md"), ("KNOTES_PROBE_NOTE_SCROLL", "end")]);
    let visible = out.lines().find_map(|l| l.strip_prefix("note-visible: ")).unwrap_or_else(|| panic!("{out}"));
    let (top, bottom): (f64, f64) = {
        let mut p = visible.split(' ').map(|v| v.parse::<f64>().unwrap());
        (p.next().unwrap(), p.next().unwrap())
    };
    let layers = formula_images(&out);
    assert!(top > 1000.0, "scrolled down: {visible}");
    assert!(!layers.is_empty() && layers.len() < 100, "only the formulas in view: {}", layers.len());
    assert!(layers.iter().all(|l| l.1 > top - 400.0 && l.1 < bottom + 400.0), "{visible}: {layers:?}");
}

#[test]
fn a_wide_inline_formula_at_a_line_end_moves_down_whole_instead_of_overflowing() {
    let note = "Енергія спокою $E = mc^2$, теорема Піфагора $a^2 + b^2 = c^2$, а корені квадратного рівняння дорівнюють $x_{1,2} = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}$.\n\nend";
    let (_vault, out) = probe_notes(&[("q.md", note)], &[("KNOTES_PROBE_OPEN", "q.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let layout = out.lines().find_map(|l| l.strip_prefix("text-layout: ")).unwrap();
    let field = |name: &str| -> f64 { layout.split(' ').find_map(|p| p.strip_prefix(name)).unwrap().parse().unwrap() };
    let right = field("left=") + field("width=");
    let layers = formula_images(&out);
    assert_eq!(layers.len(), 3, "{out}");
    assert!(layers.iter().all(|l| l.0 + l.2 <= right + 1.0), "inside the text column (right edge {right}): {layers:?}");
}

#[test]
fn struck_text_is_drawn_with_a_line_through_it_and_its_tildes_are_not() {
    let note = "keep ~~gone **bold** too~~ here\n\nend";
    let (_vault, out) = probe_notes(&[("s.md", note)], &[("KNOTES_PROBE_OPEN", "s.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let struck = out.lines().find_map(|l| l.strip_prefix("editor-struck: ")).unwrap_or_else(|| panic!("{out}"));
    assert_eq!(struck, "gone |bold| too", "the hidden ** between: {out}");
}

/// `decoration: kind x y w h` lines of the probe (bullets, boxes and bars drawn in view).
fn decorations(out: &str, kind: &str) -> Vec<(f64, f64, f64, f64)> {
    out.lines()
        .filter_map(|l| l.strip_prefix("decoration: "))
        .filter_map(|l| l.strip_prefix(kind).and_then(|rest| rest.strip_prefix(' ')))
        .map(|l| {
            let p: Vec<f64> = l.split(' ').map(|v| v.parse().unwrap()).collect();
            (p[0], p[1], p[2], p[3])
        })
        .collect()
}

#[test]
fn list_bullets_show_as_dots_indented_by_depth_and_the_marker_shows_with_the_cursor_on_it() {
    let note = "- one\n\t- two\n- three\n\nend";
    let (_vault, out) = probe_notes(&[("l.md", note)], &[("KNOTES_PROBE_OPEN", "l.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.starts_with("[bullet]- |[plain]one⏎|[indent]\t|[bullet]- |"), "{runs}");
    let dots = decorations(&out, "bullet");
    assert_eq!(dots.len(), 3, "{out}");
    assert!(dots[1].0 > dots[0].0 + 10.0 && (dots[2].0 - dots[0].0).abs() < 0.5, "nested one indented: {dots:?}");
    assert!(dots.iter().all(|d| d.2 > 3.0 && d.2 < 8.0), "{dots:?}");
    let (_vault, out) = probe_notes(&[("l.md", note)], &[("KNOTES_PROBE_OPEN", "l.md"), ("KNOTES_PROBE_CURSOR", "0")]);
    assert!(editor_runs(&out).starts_with("[list-marker]- |[plain]one⏎|[indent]\t|[bullet]- |"), "{out}");
    assert_eq!(decorations(&out, "bullet").len(), 2, "{out}");
}

#[test]
fn tasks_show_as_boxes_done_ones_struck_and_a_click_on_a_box_ticks_it() {
    let note = "- [ ] buy milk\n- [x] call mom\n\nend";
    let (_vault, out) = probe_notes(&[("t.md", note)], &[("KNOTES_PROBE_OPEN", "t.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.starts_with("[task]- [ ] |[plain]buy milk⏎|[task]- [x] |[done]call mom|"), "{runs}");
    assert_eq!(decorations(&out, "task").len(), 1, "{out}");
    assert_eq!(decorations(&out, "task-done").len(), 1, "{out}");
    assert_eq!(out.lines().find_map(|l| l.strip_prefix("editor-struck: ")), Some("call mom"), "{out}");
    let (_vault, out) = probe_notes(&[("t.md", note)], &[("KNOTES_PROBE_OPEN", "t.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "3")]);
    assert!(out.contains("note: t.md | - [x] buy milk⏎- [x] call mom"), "{out}");
    assert!(out.contains("cursor: 34"), "the cursor stays: {out}");
    let (_vault, out) = probe_notes(&[("t.md", note)], &[("KNOTES_PROBE_OPEN", "t.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "18")]);
    assert!(out.contains("note: t.md | - [ ] buy milk⏎- [ ] call mom"), "{out}");
}

#[test]
fn quotes_hide_their_markers_and_show_a_bar_per_level() {
    let note = "> said once\n> > and again\n\nend";
    let (_vault, out) = probe_notes(&[("q.md", note)], &[("KNOTES_PROBE_OPEN", "q.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.starts_with("[quote]> |[plain]said once⏎|[quote]> > |[plain]and again⏎"), "{runs}");
    let bars = decorations(&out, "quote");
    assert_eq!(bars.len(), 3, "one bar on the first line, two on the second: {out}");
    assert!(bars[2].0 > bars[1].0 + 10.0 && (bars[1].0 - bars[0].0).abs() < 0.5, "{bars:?}");
    assert!(bars[0].3 >= 17.0 && bars[0].1 + bars[0].3 >= bars[1].1 - 0.5, "bars span their lines and join: {bars:?}");
    let (_vault, out) = probe_notes(&[("q.md", "> a\n>\n> b\n\nend")], &[("KNOTES_PROBE_OPEN", "q.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert!(decorations(&out, "quote").iter().all(|b| b.3 >= 17.0), "an empty quote line keeps its height: {out}");
    let (_vault, out) = probe_notes(&[("q.md", note)], &[("KNOTES_PROBE_OPEN", "q.md"), ("KNOTES_PROBE_CURSOR", "3")]);
    assert!(editor_runs(&out).starts_with("[quote-marker]> |[plain]said once⏎|[quote]> > "), "{out}");
}

#[test]
fn a_list_in_a_quote_keeps_the_quote_indentation() {
    let note = "> - item\n\nend";
    let (_vault, out) = probe_notes(&[("q.md", note)], &[("KNOTES_PROBE_OPEN", "q.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let (bar, dot) = (decorations(&out, "quote"), decorations(&out, "bullet"));
    assert!(bar.len() == 1 && dot.len() == 1 && dot[0].0 > bar[0].0 + 10.0, "the bullet right of the bar: {bar:?} {dot:?}");
}

#[test]
fn code_blocks_get_a_background_and_fold_their_fences_away_from_the_cursor() {
    let note = "text\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\nend";
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[fence]```rust|[codeblock]⏎let x = 1;⏎let y = 2;⏎|[fence]```|"), "{runs}");
    let back = decorations(&out, "code");
    assert_eq!(back.len(), 4, "fence, two lines, fence: {out}");
    assert!(back.windows(2).all(|w| (w[0].1 + w[0].3 - w[1].1).abs() < 0.5 && w[0].0 == w[1].0), "one block: {back:?}");
    assert!(back[0].3 < 12.0 && back[3].3 < 12.0 && back[1].3 >= 17.0, "fences fold to padding: {back:?}");
    let inside = note.find("let y").unwrap().to_string();
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", &inside)]);
    assert!(editor_runs(&out).contains("[code-fence]```rust|[codeblock]⏎let x = 1;⏎let y = 2;⏎|[code-fence]```|"), "{out}");
    assert!(decorations(&out, "code").iter().all(|b| b.3 >= 17.0), "fences show as lines: {out}");
}

#[test]
fn typing_a_space_after_empty_brackets_in_a_list_item_makes_a_task() {
    let (_vault, out) =
        probe_notes(&[("t.md", "- [ ] one\n")], &[("KNOTES_PROBE_OPEN", "t.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_TYPE", "- [] two"), ("KNOTES_PROBE_TYPE_CHARS", "1")]);
    assert!(out.contains("note: t.md | - [ ] one⏎- [ ] two"), "{out}");
    assert!(editor_runs(&out).contains("[task]- [ ] |[plain]two"), "{out}");
    assert!(out.contains("cursor: 19"), "the cursor after what was typed: {out}");
    let (_vault, out) = probe_notes(&[("t.md", "")], &[("KNOTES_PROBE_OPEN", "t.md"), ("KNOTES_PROBE_TYPE", "- [] pasted")]);
    assert!(out.contains("note: t.md | - [] pasted"), "a paste stays as it is: {out}");
}

#[test]
fn an_empty_list_item_keeps_a_line_of_its_own() {
    let note = "- [ ] first\n\t- [ ] \n- \n\nend";
    let (_vault, out) = probe_notes(&[("e.md", note)], &[("KNOTES_PROBE_OPEN", "e.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let tasks = decorations(&out, "task");
    let bullet = decorations(&out, "bullet");
    assert_eq!((tasks.len(), bullet.len()), (2, 1), "{out}");
    assert!(tasks[1].1 - tasks[0].1 >= 17.0, "the empty task a line below: {tasks:?}");
    assert!(bullet[0].1 - tasks[1].1 >= 17.0, "the empty bullet a line below that: {tasks:?} {bullet:?}");
}

#[test]
fn an_empty_list_item_or_quote_line_sits_on_the_same_baseline_as_its_neighbours() {
    let note = "- [ ] first\n- [ ] \n- [ ] third\n\n> a\n>\n> b\n\nend";
    let (_vault, out) = probe_notes(&[("e.md", note)], &[("KNOTES_PROBE_OPEN", "e.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let tasks = decorations(&out, "task");
    assert!(tasks.windows(2).all(|w| (w[1].1 - w[0].1 - 18.0).abs() < 0.5), "one line apart each: {tasks:?}");
    let bars = decorations(&out, "quote");
    assert!(bars.windows(2).all(|w| (w[1].1 - w[0].1 - 18.0).abs() < 0.5 && (w[0].3 - 18.0).abs() < 0.5), "{bars:?}");
}

#[test]
fn a_task_circle_is_centred_on_capital_letters_higher_than_a_bullet_dot() {
    let (_vault, out) = probe_notes(&[("c.md", "- [ ] Task\n- Bullet\n\nend")], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let (task, dot) = (decorations(&out, "task")[0], decorations(&out, "bullet")[0]);
    let (task_centre, dot_centre_a_line_up) = (task.1 + task.3 / 2.0, dot.1 + dot.3 / 2.0 - 18.0);
    assert!(dot_centre_a_line_up - task_centre > 1.0, "the circle sits higher in its line: {task:?} {dot:?}");
}

const RENAME_VAULT: [(&str, &str); 3] = [("Old.md", "# Old\n\ntext"), ("a.md", "see [[Old]] and [[Old#Part|p]]"), ("b.md", "[x](Old.md)")];

fn renamed(answer: &str) -> (tempfile::TempDir, String) {
    probe_notes(&RENAME_VAULT, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "rename=New"), ("KNOTES_PROBE_ANSWER", answer)])
}

#[test]
fn renaming_a_note_asks_and_then_moves_it_and_updates_the_links_to_it() {
    let (vault, out) = renamed("update");
    assert!(out.contains("rename-question: 3 2"), "{out}");
    let read = |p: &str| std::fs::read_to_string(vault.path().join(p)).ok();
    assert_eq!(read("Old.md"), None, "{out}");
    assert_eq!(read("New.md").as_deref(), Some("# Old\n\ntext"));
    assert_eq!(read("a.md").as_deref(), Some("see [[New]] and [[New#Part|p]]"));
    assert_eq!(read("b.md").as_deref(), Some("[x](New.md)"));
    assert!(out.contains("note: New.md"), "the open note follows its file: {out}");
}

#[test]
fn a_rename_can_keep_the_links_or_be_cancelled() {
    let (vault, out) = renamed("keep");
    let read = |p: &str| std::fs::read_to_string(vault.path().join(p)).ok();
    assert!(read("New.md").is_some() && read("a.md").as_deref() == Some("see [[Old]] and [[Old#Part|p]]"), "{out}");
    let (vault, out) = renamed("cancel");
    let read = |p: &str| std::fs::read_to_string(vault.path().join(p)).ok();
    assert!(read("Old.md").is_some() && read("New.md").is_none(), "{out}");
}

#[test]
fn a_note_is_renamed_in_its_list_row_from_the_context_menu() {
    let (vault, out) = probe_notes(
        &RENAME_VAULT,
        &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "list-rename=Renamed in list"), ("KNOTES_PROBE_ANSWER", "update")],
    );
    assert!(out.contains("list-menu-items: Перейменувати"), "{out}");
    assert!(vault.path().join("Renamed in list.md").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "see [[Renamed in list]] and [[Renamed in list#Part|p]]");
}

#[test]
fn a_note_is_moved_to_a_folder_picked_from_the_context_menu() {
    let notes = [("Old.md", "# Old"), ("a.md", "[x](Old.md) [[Old]]"), ("Archive/keep.md", "k")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "list-move=Archive")]);
    assert!(out.contains("list-menu-items: Перейменувати | Перемістити в…"), "{out}");
    assert!(out.contains("move-folders: / | Archive"), "{out}");
    assert!(vault.path().join("Archive/Old.md").exists() && !vault.path().join("Old.md").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "[x](Archive/Old.md) [[Old]]");
    assert!(out.contains("note: Archive/Old.md"), "{out}");
}

#[test]
fn a_note_dragged_onto_a_sidebar_folder_moves_there() {
    let notes = [("Old.md", "# Old"), ("a.md", "[x](Old.md)"), ("Archive/keep.md", "k")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "drag-to=Archive")]);
    assert!(out.contains("drop-targets: Archive=move All notes=none"), "{out}");
    assert!(vault.path().join("Archive/Old.md").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "[x](Archive/Old.md)");
}

#[test]
fn a_folder_is_renamed_from_the_sidebar_with_the_links_into_it() {
    let notes = [
        ("Archive/a.md", "[b](b.md) [[Archive/b]]"),
        ("Archive/b.md", "x"),
        ("Archive/img.png", "png"),
        ("c.md", "[[Archive/a]] [a](Archive/a.md) [[b]] ![[Archive/img.png]]"),
    ];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Archive/a.md"), ("KNOTES_PROBE_ACTION", "folder-rename=Archive=Old stuff")]);
    assert!(out.contains("sidebar-menu-items: Перейменувати"), "{out}");
    assert!(out.contains("rename-question: 4 2"), "{out}");
    let read = |p: &str| std::fs::read_to_string(vault.path().join(p)).ok();
    assert!(read("Archive/a.md").is_none() && read("Old stuff/img.png").as_deref() == Some("png"), "{out}");
    assert_eq!(read("c.md").as_deref(), Some("[[Old stuff/a]] [a](Old%20stuff/a.md) [[b]] ![[Old stuff/img.png]]"));
    assert_eq!(read("Old stuff/a.md").as_deref(), Some("[b](b.md) [[Old stuff/b]]"));
    assert!(out.contains("note: Old stuff/a.md"), "the open note follows: {out}");
}

#[test]
fn the_toolbar_shows_the_open_notes_name_and_renames_it() {
    let (_vault, out) = probe_notes(&RENAME_VAULT, &[("KNOTES_PROBE_OPEN", "Old.md")]);
    assert!(out.contains("note-title: Old"), "{out}");
    assert!(out.contains("toolbar: toggle-sidebar, sidebar-mode(2), sidebar-separator, search, list-options, list-separator, new-note, history(2), note-title,"), "{out}");
    let (vault, out) = probe_notes(&RENAME_VAULT, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "toolbar-rename=From toolbar")]);
    assert_eq!(std::fs::read_to_string(vault.path().join("From toolbar.md")).ok().as_deref(), Some("# Old\n\ntext"), "the first line is content, not the name: {out}");
    assert!(out.contains("note-title: From toolbar"), "{out}");
}

#[test]
fn links_and_task_circles_show_a_hand_where_a_click_acts() {
    let note = "T\n\nsee [[b]] here\n\n- [ ] task\n\nplain";
    let hover = |at: &str, cursor: &str| {
        let (_vault, out) = probe_notes(&[("a.md", note), ("b.md", "b")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", cursor), ("KNOTES_PROBE_HOVER", at)]);
        out.lines().find_map(|l| l.strip_prefix("hover: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
    };
    assert_eq!(hover("9", "end"), "hand", "a link away from the cursor");
    assert_eq!(hover("9", "3"), "text", "in the cursor's line a plain click places the cursor");
    assert_eq!(hover("20", "end"), "hand", "a task circle");
    assert_eq!(hover("30", "end"), "text");
}

#[test]
fn cmd_f_finds_in_the_note_and_cmd_shift_f_searches_the_notes() {
    let (_vault, out) = probe_notes(&[("a.md", "text"), ("b.md", "more")], &[("KNOTES_PROBE_OPEN", "a.md")]);
    assert!(out.contains("Знайти… ⌘F | Знайти далі ⌘G | Знайти попереднє ⇧⌘G | Пошук у нотатках ⇧⌘F"), "{out}");
    // From the list, too: Cmd+F always finds in the open note.
    let (_vault, out) = probe_notes(&[("a.md", "text"), ("b.md", "more")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_MENU", "Знайти…")]);
    assert!(out.contains("find-bar: shown") && out.contains("focus: find"), "{out}");
    let (_vault, out) = probe_notes(&[("a.md", "text"), ("b.md", "more")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_MENU", "Пошук у нотатках")]);
    assert!(out.contains("search-row: shown") && out.contains("focus: search"), "{out}");
}

#[test]
fn a_note_opened_from_a_content_search_shows_its_matches_scrolled_into_view() {
    let long: String = (0..200).map(|i| format!("Рядок {i} звичайного тексту.\n\n")).collect::<String>() + "Тут шуканий радар нарешті.\n\nІ ще радари тут.";
    let notes = [("a.md", long.as_str()), ("b.md", "інше")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_SEARCH", "content: радар"), ("KNOTES_PROBE_OPEN", "a.md")]);
    assert!(out.contains("found-in-note: 2"), "{out}");
    let visible = out.lines().find_map(|l| l.strip_prefix("note-visible: ")).unwrap();
    assert!(visible.split(' ').next().unwrap().parse::<f64>().unwrap() > 1000.0, "scrolled to the first match: {visible}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md")]);
    assert!(out.contains("found-in-note: 0"), "nothing found without a search: {out}");
}

#[test]
fn inline_code_has_a_background_and_its_backticks_do_not() {
    let (_vault, out) = probe_notes(&[("c.md", "T\n\nrun `cargo test` now\n\nend")], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert_eq!(out.lines().find_map(|l| l.strip_prefix("editor-shaded: ")), Some("cargo test"), "{out}");
}

#[test]
fn a_click_on_a_completion_row_inserts_that_note() {
    let notes = [("Note.md", "n"), ("A/Note.md", "n"), ("a.md", "")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[Note"), ("KNOTES_PROBE_COMPLETION_CLICK", "1")]);
    assert!(out.contains("note: a.md | [[A/Note]]"), "{out}");
    assert_eq!(completion(&out), "none");
}

#[test]
fn after_hash_caret_the_list_offers_the_targets_block_anchors() {
    let notes = [("b.md", "First paragraph ^p1\n\nSecond one ^two"), ("a.md", "")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[b#^")]);
    assert_eq!(completion(&out), "▸^p1 — First paragraph | ^two — Second one", "{out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "[[b#^t"), ("KNOTES_PROBE_KEYS", "return")]);
    assert!(out.contains("note: a.md | [[b#^two]]"), "{out}");
}

#[test]
fn formulas_take_the_text_colour_of_a_new_appearance() {
    let (_vault, out) = probe_notes(&[("f.md", FORMULA_NOTE)], &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert!(out.contains("formula-ink: light"), "{out}");
    let (_vault, out) = probe_notes(&[("f.md", FORMULA_NOTE)], &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_APPEARANCE", "dark")]);
    assert!(out.contains("formula-ink: dark"), "{out}");
    assert_eq!(formula_images(&out).len(), 2, "still typeset: {out}");
}

#[test]
fn links_to_websites_are_marked_with_an_arrow_and_links_inside_the_vault_are_not() {
    let note = "T\n\nsee [site](https://example.com) and [[b]] and [local](b.md)\n\nend";
    let (_vault, out) = probe_notes(&[("a.md", note), ("b.md", "b")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let arrows = decorations(&out, "external");
    assert_eq!(arrows.len(), 1, "{out}");
    let (_vault, out) = probe_notes(&[("a.md", note), ("b.md", "b")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "5")]);
    assert!(decorations(&out, "external").is_empty(), "the source shows in the cursor's line: {out}");
}

#[test]
fn a_bare_web_address_is_a_link_with_an_arrow() {
    let note = "T\n\nsee https://example.com here\n\nends with https://x.y";
    let (_vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_HOVER", "10")]);
    assert!(editor_runs(&out).contains("[link]https://example.com|"), "{out}");
    assert_eq!(decorations(&out, "external").len(), 2, "after the address, mid-line or at the end of one: {out}");
    assert!(out.contains("hover: hand"), "{out}");
}

#[test]
fn find_is_enabled_in_the_menu_while_the_note_has_the_focus() {
    let (_vault, out) = probe_notes(&[("a.md", "text")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert!(out.contains("find-menu: Знайти…=on Знайти далі=on Знайти попереднє=on"), "Cmd+F must work from the text: {out}");
    for focus in ["note", "list"] {
        let (_vault, out) =
            probe_notes(&[("a.md", "text")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_SHORTCUT", "f"), ("KNOTES_PROBE_SHORTCUT_FOCUS", focus)]);
        assert!(out.contains("shortcut-handled: true") && out.contains("find-bar: shown"), "Cmd+F from the {focus}: {out}");
    }
}

#[test]
fn find_highlights_every_occurrence_selects_one_and_steps_through_them() {
    let note = "Пункт один.\n\nДругий пункт і ще ПУНКТ.\n\nкінець";
    let (_vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_FIND", "пункт")]);
    assert!(out.contains("find: 1 з 3") && out.contains("found-in-note: 3"), "{out}");
    assert!(out.contains("cursor: 0"), "the first occurrence selected: {out}");
    let (_vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_FIND", "пункт"), ("KNOTES_PROBE_FIND_STEPS", "3")]);
    assert!(out.contains("find: 1 з 3"), "round the note: {out}");
    let (_vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_FIND", "немає такого")]);
    assert!(out.contains("find: Немає") && out.contains("found-in-note: 0"), "{out}");
}

#[test]
fn callouts_show_as_tinted_boxes_with_an_icon_and_a_title() {
    let note = "T\n\n> [!warning] Mind the gap\n> Text inside.\n\n> [!tip]\n> No title.\n\nend";
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[quote]> |[callout][!warning] |[callout-title]Mind the gap|"), "{runs}");
    assert_eq!(decorations(&out, "callout-box").len(), 4, "two lines in each box: {out}");
    assert_eq!(decorations(&out, "callout-icon").len(), 2, "{out}");
    assert!(decorations(&out, "quote").is_empty(), "no quote bars in a callout: {out}");
    let inside = (note.find("Mind").unwrap()).to_string();
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", &inside)]);
    assert!(editor_runs(&out).contains("[callout-marker][!warning] |[callout-title]Mind the gap"), "the marker shows on its line: {out}");
}

#[test]
fn the_window_opens_where_and_as_big_as_it_was_left() {
    let notes = tempfile::tempdir().unwrap();
    std::fs::write(notes.path().join("a.md"), "a").unwrap();
    let home = tempfile::tempdir().unwrap();
    probe_env(&[notes.path()], Some(home.path()), &[("KNOTES_PROBE_WIDTH", "1180"), ("KNOTES_PROBE_ACTION", "close")]);
    let out = probe_env(&[notes.path()], Some(home.path()), &[]);
    assert!(out.contains("size: 1180x"), "{out}");
}

#[test]
fn the_list_gets_its_width_back_when_the_sidebar_comes_back() {
    let (_vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "toggle-sidebar-twice"), ("KNOTES_PROBE_DELAY_MS", "2500")]);
    let widths: Vec<f64> = out.lines().find_map(|l| l.strip_prefix("widths: ")).unwrap().split(' ').map(|w| w.parse().unwrap()).collect();
    assert!((widths[1] - 340.0).abs() < 2.0, "the list as wide as before the sidebar went: {widths:?}");
}

const PIC: &[u8] = include_bytes!("fixtures/pic-120x80.png");
const WIDE: &[u8] = include_bytes!("fixtures/wide-1600x400.png");

fn probe_images(notes: &[(&str, &str)], images: &[(&str, &[u8])], env: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let vault = tempfile::tempdir().unwrap();
    for (path, bytes) in notes.iter().map(|(p, t)| (*p, t.as_bytes())).chain(images.iter().copied()) {
        let file = vault.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    let out = probe_env(&[vault.path()], None, env);
    (vault, out)
}

/// `image: x y w h kind` lines, as numbers and kind.
fn images(out: &str) -> Vec<(f64, f64, f64, f64, String)> {
    out.lines()
        .filter_map(|l| l.strip_prefix("image: "))
        .map(|l| {
            let p: Vec<&str> = l.split(' ').collect();
            (p[0].parse().unwrap(), p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap(), p[4].to_owned())
        })
        .collect()
}

#[test]
fn an_image_alone_on_its_line_shows_centred_at_its_size() {
    let (_vault, out) = probe_images(
        &[("a.md", "Before\n![[pic.png]]\nAfter")],
        &[("att/pic.png", PIC)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")],
    );
    let shown = images(&out);
    assert_eq!(shown.len(), 1, "{out}");
    let (x, _, w, h, kind) = &shown[0];
    assert_eq!((*w, *h, kind.as_str()), (120.0, 80.0, "shown"), "{out}");
    let widths: Vec<f64> = out.lines().find_map(|l| l.strip_prefix("widths: ")).unwrap().split(' ').map(|w| w.parse().unwrap()).collect();
    assert!((x + w / 2.0 - widths[2] / 2.0).abs() < 12.0, "centred in the note column ({}): {out}", widths[2]);
}

#[test]
fn an_image_takes_the_width_written_after_the_bar_and_no_more_than_the_column() {
    let (_vault, out) = probe_images(
        &[("a.md", "![[pic.png|60]]\n![[wide.png]]\nend")],
        &[("pic.png", PIC), ("wide.png", WIDE)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")],
    );
    let shown = images(&out);
    assert_eq!(shown.len(), 2, "{out}");
    assert_eq!((shown[0].2, shown[0].3), (60.0, 40.0), "{out}");
    let widths: Vec<f64> = out.lines().find_map(|l| l.strip_prefix("widths: ")).unwrap().split(' ').map(|w| w.parse().unwrap()).collect();
    let (w, h) = (shown[1].2, shown[1].3);
    assert!(w < widths[2] - 40.0 && w > 300.0, "as wide as the column at most: {w} of {}", widths[2]);
    assert!((w / h - 4.0).abs() < 0.05, "keeps its proportions: {w} x {h}");
}

#[test]
fn with_the_cursor_on_its_line_an_image_shows_its_source_and_stays_below() {
    let (_vault, out) = probe_images(
        &[("a.md", "Before\n![[pic.png]]\nAfter")],
        &[("pic.png", PIC)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "10")],
    );
    let shown = images(&out);
    assert_eq!(shown.iter().map(|i| i.4.as_str()).collect::<Vec<_>>(), ["preview"], "{out}");
    assert!(out.contains("[wikilink]![[pic.png]]") || out.contains("[markup]![["), "the source shows: {out}");
}

#[test]
fn a_click_on_an_image_puts_the_cursor_in_its_source_and_opens_nothing() {
    let (_vault, out) = probe_images(
        &[("a.md", "Before\n![[pic.png]]\nAfter")],
        &[("pic.png", PIC)],
        // Character 7 keeps the image's width: its middle is the image's.
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "7")],
    );
    assert!(out.contains("note: a.md"), "{out}");
    assert_eq!(images(&out).iter().map(|i| i.4.as_str()).collect::<Vec<_>>(), ["preview"], "{out}");
}

#[test]
fn a_missing_image_stays_a_link() {
    let (_vault, out) = probe_images(&[("a.md", "![[gone.png]]\nend")], &[], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert!(images(&out).is_empty(), "{out}");
}

#[test]
fn an_image_as_wide_as_the_column_takes_one_line() {
    let (_vault, out) = probe_images(
        &[("a.md", "![[wide.png]]\n![[wide.png]]\nend")],
        &[("wide.png", WIDE)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")],
    );
    let shown = images(&out);
    assert_eq!(shown.len(), 2, "{out}");
    let step = shown[1].1 - shown[0].1;
    assert!((step - (shown[0].3 + 12.0)).abs() < 1.5, "one line each, as tall as the image and its gaps: {step} {out}");
}

fn attachments(vault: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(vault.join(".knotes/attachments"))
        .map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_pasted_image_goes_to_the_attachments_and_shows_in_the_note() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pic-120x80.png");
    let (vault, out) = probe_images(
        &[("a.md", "Text")],
        &[],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_PASTE", fixture), ("KNOTES_PROBE_DELAY_MS", "2500")],
    );
    let names = attachments(vault.path());
    assert_eq!(names.len(), 1, "{out}");
    assert!(names[0].starts_with("Pasted image 2") && names[0].ends_with(".png"), "{names:?}");
    assert_eq!(std::fs::read(vault.path().join(".knotes/attachments").join(&names[0])).unwrap(), PIC);
    assert!(out.contains(&format!("note: a.md | Text⏎![[{}]]", names[0])), "{out}");
    assert_eq!(images(&out).len(), 1, "the image shows once indexed: {out}");
}

#[test]
fn dropped_image_files_are_copied_and_embedded_where_they_fall() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("photo.png"), PIC).unwrap();
    std::fs::write(source.path().join("notes.txt"), "x").unwrap();
    let drop = format!("{},{}@0", source.path().join("photo.png").display(), source.path().join("notes.txt").display());
    let (vault, out) = probe_images(&[("a.md", "Text")], &[], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_DROP", &drop)]);
    assert_eq!(attachments(vault.path()), ["photo.png"], "{out}");
    assert!(out.contains("note: a.md | ![[photo.png]]⏎Text"), "{out}");
}

#[test]
fn an_image_dropped_from_the_vault_is_embedded_where_it_is() {
    let vault_files = tempfile::tempdir().unwrap();
    let vault_root = vault_files.path().canonicalize().unwrap();
    std::fs::write(vault_root.join("a.md"), "Text").unwrap();
    std::fs::create_dir(vault_root.join("att")).unwrap();
    std::fs::write(vault_root.join("att/pic.png"), PIC).unwrap();
    let drop = format!("{}@4", vault_root.join("att/pic.png").display());
    let out = probe_env(&[&vault_root], None, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_DROP", &drop)]);
    assert!(attachments(&vault_root).is_empty(), "nothing copied: {out}");
    assert!(out.contains("note: a.md | Text⏎![[pic.png]]"), "{out}");
}

#[test]
fn an_image_pasted_with_text_pastes_the_text() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pic-120x80.png");
    let (vault, out) = probe_images(
        &[("a.md", "Text")],
        &[],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_PASTE", fixture), ("KNOTES_PROBE_PASTE_TEXT", "cell")],
    );
    assert!(attachments(vault.path()).is_empty(), "{out}");
    assert!(out.contains("pasted-images: no"), "{out}");
}

#[test]
fn the_pointer_over_an_image_s_right_edge_resizes_it() {
    let notes = [("a.md", "Before\n![[pic.png]]\nAfter")];
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")];
    let (_vault, out) = probe_images(&notes, &[("pic.png", PIC)], &env);
    let (x, y, w, h, _) = images(&out)[0].clone();
    let points = format!("{},{};{},{}", x + w - 2.0, y + h / 2.0, x + w / 2.0, y + h / 2.0);
    let (_vault, out) = probe_images(&notes, &[("pic.png", PIC)], &[env[0], env[1], ("KNOTES_PROBE_EDGE", &points)]);
    assert!(out.contains("image-edge: resize none"), "{out}");
}

#[test]
fn a_resized_image_writes_its_width_and_shows_at_it() {
    let (_vault, out) = probe_images(
        &[("a.md", "Before\n![[pic.png]]\nAfter")],
        &[("pic.png", PIC)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_RESIZE", "7:200")],
    );
    assert!(out.contains("note: a.md | Before⏎![[pic.png|200]]⏎After"), "{out}");
    let shown = images(&out);
    assert_eq!((shown[0].2, shown[0].3, shown[0].4.as_str()), (200.0, 133.0, "shown"), "{out}");
}
