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

/// The time every launch of this run takes for now (seconds since 1970): when the run began.
fn run_now() -> i64 {
    static NOW: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *NOW.get_or_init(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
}

fn probe_env(args: &[&std::path::Path], home: Option<&std::path::Path>, env: &[(&str, &str)]) -> String {
    let scratch_home = tempfile::tempdir().unwrap();
    // One time for every launch of the run: a card graded in one launch is due «in ten
    // minutes» in the next, whatever the Mac's clock did between them (a run that slept
    // found such cards due and failed).
    let now = &run_now().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes"))
        .args(args)
        .env("KNOTES_PROBE_NOW", now)
        .envs(env.iter().copied())
        .env("KNOTES_PROBE", "1")
        .env("HOME", home.unwrap_or(scratch_home.path()))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("app starts");
    // Read while the app runs: a report longer than the pipe's buffer would block it.
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut out).unwrap();
        out
    });
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let out = reader.join().unwrap();
            assert!(status.success(), "app failed: {out}");
            return out;
        }
        // A cold index of the 50k vault takes about 20 s alone, longer beside other tests.
        if start.elapsed() > Duration::from_secs(60) {
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
        "toggle-sidebar, space, tabs, space, toolbar-search, toolbar-new-note, toolbar-zen, space, panel",
        "the sidebar's button, the tabs, the search and a new note together, the panel's button"
    );
    // What belongs to the list and the note is in the row under the toolbar, each part over its
    // column.
    let row = out.lines().find_map(|l| l.strip_prefix("second-row: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(row.contains(" [search] [list-options] | [back] [forward] "), "a new note's button is in the toolbar, not here too: {row}");
    let parts = out.lines().find_map(|l| l.strip_prefix("second-row-parts: ")).unwrap_or_else(|| panic!("{out}"));
    let edges: Vec<f64> = parts.split([' ', '+']).filter_map(|n| n.parse().ok()).collect();
    // The row spans the window: the list's part starts where the sidebar ends (it begins under
    // the row too), the note's where the list's ends.
    assert!(edges.len() == 4 && edges[0] >= 200.0 && (edges[2] - edges[0] - edges[1]).abs() <= 2.0 && edges[1] > 200.0 && edges[3] > 300.0, "the list's part, then the note's: {parts}");
    let widths = out.lines().find_map(|l| l.strip_prefix("widths: ")).unwrap_or_else(|| panic!("{out}"));
    let w: Vec<f64> = widths.split(' ').map(|x| x.parse().unwrap()).collect();
    // The owner's default widths (2026-11-17): sidebar, list, the note with the rest, panel.
    assert!(w[0] == 233.0 && w[1] == 282.0 && w[3] == 317.0 && w[2] >= 400.0, "{widths}");
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
fn the_chat_asks_to_sign_in_with_chatgpt_first() {
    let out = probe();
    assert!(out.contains("chat: signed out | Continue with ChatGPT | \n"), "{out}");
}

#[test]
fn main_menu_has_the_standard_menus() {
    let out = probe();
    assert!(out.contains("menu: Knotes | Файл | Редагування | Вигляд | Вікно"), "{out}");
    assert!(out.contains("file-menu: Нова нотатка ⌘N | Нова тека ⇧⌘N | Відкрити базу… ⌘O | Бази… ⇧⌘O | Вкладення… | Зберегти ⌘S | Закрити вкладку ⌘W | Закрити вікно ⇧⌘W"), "{out}");
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
/// The app run on `notes` with `home` as HOME: settings one launch writes, the next reads.
fn probe_notes_at_home(notes: &[(&str, &str)], env: &[(&str, &str)], home: &std::path::Path) -> (tempfile::TempDir, String) {
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in notes {
        std::fs::write(vault.path().join(path), text).unwrap();
    }
    let out = probe_env(&[vault.path()], Some(home), env);
    (vault, out)
}

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
fn a_plain_click_in_the_link_the_cursor_is_in_places_the_cursor_and_cmd_click_opens() {
    let notes = [("a.md", "see [[b]] here"), ("b.md", "target")];
    // The cursor in the link: its source shows, and a click there is a click in text.
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "5"), ("KNOTES_PROBE_CLICK", "6")]);
    assert!(out.contains("note: a.md"), "{out}");
    assert!(out.contains("cursor: 6") || out.contains("cursor: 7"), "{out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "5"), ("KNOTES_PROBE_CLICK", "6:cmd")]);
    assert!(out.contains("note: b.md | target"), "{out}");
    // The cursor elsewhere in the line: the link reads as a link, and a click follows it.
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_CLICK", "6")]);
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
fn cmd_n_creates_an_untitled_note_in_the_selected_folder_and_focuses_its_name() {
    let (vault, out) = probe_notes(&[("Projects/a.md", "a")], &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_ACTION", "new")]);
    assert!(vault.path().join("Projects/Без назви.md").is_file(), "{out}");
    assert!(out.contains("note: Projects/Без назви.md"), "{out}");
    assert!(out.contains("note-title-focus: editing\n"), "its name first, to type (owner's request): {out}");
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
    assert!(out.contains("sidebar-mode: folders"), "every launch starts on the folders (owner's choice): {out}");
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

#[test]
fn the_panel_switch_shows_the_ai_or_the_outline_and_is_remembered() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    let home = tempfile::tempdir().unwrap();
    let out = probe_env(&[vault.path()], Some(home.path()), &[]);
    assert!(out.contains("panel: ai, shows chat\n"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_ACTION", "panel=outline")]);
    assert!(out.contains("panel: outline, shows outline\n"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[]);
    assert!(out.contains("panel: outline, shows outline\n"), "remembered: {out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_ACTION", "panel=ai")]);
    assert!(out.contains("panel: ai, shows chat\n"), "{out}");
}

#[test]
fn the_panel_switch_hides_with_the_panel() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "toggle-panel")]);
    assert!(out.contains("panel: hidden"), "{out}");
}

/// x of where the row under the toolbar starts its part over the note, and of the list/note
/// column boundary.
fn separator_and_boundary(out: &str) -> (f64, f64) {
    let line = out.lines().find_map(|l| l.strip_prefix("row-boundary: ")).unwrap_or_else(|| panic!("{out}"));
    let (sep, boundary) = line.split_once(" boundary ").unwrap();
    (sep.parse().unwrap(), boundary.parse().unwrap())
}

#[test]
fn the_row_under_the_toolbar_keeps_its_parts_over_their_columns_when_the_sidebar_collapses() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    for action in ["none", "toggle-sidebar"] {
        let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", action)]);
        let (sep, boundary) = separator_and_boundary(&out);
        assert!((sep - boundary).abs() <= 1.5, "{action}: the note's part at {sep}, boundary at {boundary}");
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
fn the_third_segment_searches_by_meaning() {
    let (_vault, out) = probe_notes(
        &SEARCH_NOTES,
        &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_SEARCH", "radar"), ("KNOTES_PROBE_SEARCH_MODE", "meaning")],
    );
    assert!(out.contains("search-field: meaning: radar"), "{out}");
    assert!(out.contains("search-mode: meaning"), "{out}");
    // The probe never starts the model: nothing is found, and nothing breaks.
    assert!(out.contains("subtitle: 0 нотаток"), "{out}");
    // The field stands in the place of the list's name in the row under the toolbar, the switch
    // under it across the column (beside three segments the field was a few letters wide).
    let layout = out.lines().find_map(|l| l.strip_prefix("search-layout: ")).unwrap_or_default();
    let width = |name: &str| layout.split(", ").find_map(|p| p.strip_prefix(name)).and_then(|w| w.trim().parse::<f64>().ok()).unwrap_or(0.0);
    assert!(layout.contains("switch under the field: true") && layout.ends_with("field in place of the list's name: true") && width("field ") > 200.0 && width("switch ") > width("field "), "{layout}");
    let (_vault, out) = probe_notes(&SEARCH_NOTES, &[("KNOTES_PROBE_SEARCH", "meaning: де радар")]);
    assert!(out.contains("search-mode: meaning"), "the switch follows the typed operator: {out}");
}

#[test]
fn the_settings_offer_two_models_for_search_by_meaning_or_none() {
    let (_vault, out) = probe_notes(&SEARCH_NOTES, &[("KNOTES_PROBE_SETTINGS", "1")]);
    let settings = out.lines().find(|l| l.starts_with("settings: ")).unwrap_or_default();
    assert!(settings.contains("| meaning: e5-large-instruct — точніший |"), "the default: {settings}");
    assert!(settings.ends_with("choices: e5-large-instruct — точніший / EmbeddingGemma 2 — легший / Вимкнено"), "{settings}");
    assert!(out.contains("search-meaning-segment: on"), "{out}");
}

#[test]
fn search_by_meaning_turned_off_greys_its_segment_and_stays_off() {
    let home = tempfile::tempdir().unwrap();
    let notes = [("a.md", "a")];
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS_MEANING", "off")], home.path());
    assert!(out.contains("| meaning: Вимкнено |") && out.contains("search-meaning-segment: off"), "{out}");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS", "1")], home.path());
    assert!(out.contains("| meaning: Вимкнено |") && out.contains("search-meaning-segment: off"), "kept: {out}");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS_MEANING", "embeddinggemma-2")], home.path());
    assert!(out.contains("| meaning: EmbeddingGemma 2 — легший |") && out.contains("search-meaning-segment: on"), "{out}");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS", "1")], home.path());
    assert!(out.contains("| meaning: EmbeddingGemma 2 — легший |"), "kept: {out}");
}

#[test]
fn the_settings_show_each_model_s_stored_vectors_and_clear_or_make_them_again() {
    let home = tempfile::tempdir().unwrap();
    let vaults = home.path().join("Library/Application Support/Knotes/vaults/x");
    std::fs::create_dir_all(&vaults).unwrap();
    std::fs::write(vaults.join("vectors-e5-large-instruct.sqlite"), vec![0u8; 3000]).unwrap();
    std::fs::write(vaults.join("vectors-embeddinggemma-2.sqlite"), vec![0u8; 2048]).unwrap();
    let notes = [("a.md", "a")];
    let line = |out: &str, prefix: &str| out.lines().find_map(|l| l.strip_prefix(prefix)).unwrap_or_default().to_string();
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS", "1")], home.path());
    // The model in use (e5, the default) is made again rather than cleared; the other cleared.
    assert_eq!(line(&out, "settings-cache: "), "e5-large-instruct 3 КБ [regenerate] | embeddinggemma-2 2 КБ [clear]", "{out}");
    assert_eq!(line(&out, "settings-progress: "), "Вектори актуальні", "nothing being made in the probe");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS_CLEAR", "embeddinggemma-2")], home.path());
    assert_eq!(line(&out, "settings-cache: "), "e5-large-instruct 3 КБ [regenerate] | embeddinggemma-2 немає", "{out}");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS_CLEAR", "e5-large-instruct")], home.path());
    assert_eq!(line(&out, "settings-cache: "), "e5-large-instruct немає [regenerate] | embeddinggemma-2 немає", "made again: emptied first (the probe never runs a model)");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS", "1"), ("KNOTES_PROBE_SETTINGS_MAKING", "12/40/300")], home.path());
    let making = line(&out, "settings-progress: ");
    assert!(making.starts_with("Генерація векторів: 12 з 40 нотаток · ") && making.ends_with(" блоків/с"), "{making}");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS_MEANING", "off")], home.path());
    assert_eq!(line(&out, "settings-progress: "), "Пошук за змістом вимкнено", "{out}");
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
            let p: Vec<f64> = l.split(' ').take(4).map(|v| v.parse().unwrap()).collect();
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
    // No language: the code stays one colour (colours have their own test).
    let note = "text\n\n```\nlet x = 1;\nlet y = 2;\n```\n\nend";
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[fence]```|[codeblock]⏎let x = 1;⏎let y = 2;⏎|[fence]```|"), "{runs}");
    let back = decorations(&out, "code");
    assert_eq!(back.len(), 4, "fence, two lines, fence: {out}");
    assert!(back.windows(2).all(|w| (w[0].1 + w[0].3 - w[1].1).abs() < 0.5 && w[0].0 == w[1].0), "one block: {back:?}");
    assert!(back[0].3 < 12.0 && back[3].3 < 12.0 && back[1].3 >= 17.0, "fences fold to padding: {back:?}");
    let inside = note.find("let y").unwrap().to_string();
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", &inside)]);
    assert!(editor_runs(&out).contains("[code-fence]```|[codeblock]⏎let x = 1;⏎let y = 2;⏎|[code-fence]```|"), "{out}");
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
fn a_task_circle_is_in_the_middle_of_its_line_whatever_the_room_between_lines() {
    // On the capitals' middle it sat lower than the caret's, with more room over it than under.
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("Library/Application Support/Knotes");
    std::fs::create_dir_all(&data).unwrap();
    for settings in ["{}", r#"{"noteText": {"line": 150}}"#] {
        std::fs::write(data.join("settings-v2.json"), settings).unwrap();
        let (_vault, out) = probe_notes_at_home(&[("c.md", "- [ ] Task\n- Bullet\n\nend")], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")], home.path());
        let task = decorations(&out, "task")[0];
        // The first line: 18 points under the note's top margin of 24.
        assert_eq!(task.1 + task.3 / 2.0, 24.0 + 18.0 / 2.0, "{settings}: {task:?}");
    }
}

#[test]
fn room_between_lines_stays_out_of_code_and_over_a_quote_s_bar() {
    // The room lies over each line: a quote's bar starts at its first line, under that room,
    // and goes on through the room over its next; code's lines keep their own height.
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("Library/Application Support/Knotes");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings-v2.json"), r#"{"noteText": {"line": 200}}"#).unwrap();
    let note = "text\n\n> one\n> two\n\n```\na\nb\n```\n\nend";
    let (_vault, out) = probe_notes_at_home(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")], home.path());
    let (bars, code) = (decorations(&out, "quote"), decorations(&out, "code"));
    assert_eq!((bars[0].3, bars[1].1), (18.0, bars[0].1 + 18.0), "a line tall, the next bar right under it: {bars:?}");
    assert!(bars[1].3 > 30.0, "through the room over the second line: {bars:?}");
    assert_eq!(code.iter().map(|c| c.3).collect::<Vec<_>>(), [8.0, 17.0, 17.0, 8.0], "{code:?}");
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
    assert!(out.contains("list-menu-items: Відкрити в новій вкладці | Перейменувати"), "{out}");
    assert!(vault.path().join("Renamed in list.md").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "see [[Renamed in list]] and [[Renamed in list#Part|p]]");
}

#[test]
fn a_note_is_moved_to_a_folder_picked_from_the_context_menu() {
    let notes = [("Old.md", "# Old"), ("a.md", "[x](Old.md) [[Old]]"), ("Archive/keep.md", "k")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "list-move=Archive")]);
    assert!(out.contains("list-menu-items: Відкрити в новій вкладці | Перейменувати | Перемістити в…"), "{out}");
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
    assert!(out.contains(" [forward] Old · "), "in the row under the toolbar: {out}");
    // Under it, when its file was last changed (in Ukrainian; a day long past in full).
    let (vault, _) = probe_notes(&RENAME_VAULT, &[]);
    let changed = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_741_082_700); // 2025-03-04 10:05 UTC
    std::fs::File::options().write(true).open(vault.path().join("Old.md")).unwrap().set_modified(changed).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Old.md")]);
    let date = out.lines().find_map(|l| l.strip_prefix("note-date: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(date.contains("березня 2025") && date.contains(":05"), "{date}");
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
    assert_eq!(hover("9", "3"), "hand", "in the cursor's line too, the cursor outside the link");
    assert_eq!(hover("9", "8"), "text", "in the link the cursor is in a plain click places the cursor");
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
    assert!(out.contains("find-bar: shown") && out.contains("find-field: радар\n") && out.contains("find: 1 з 2\n"), "Cmd+F's bar with the word: {out}");
    let visible = out.lines().find_map(|l| l.strip_prefix("note-visible: ")).unwrap();
    assert!(visible.split(' ').next().unwrap().parse::<f64>().unwrap() > 1000.0, "scrolled to the first match: {visible}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md")]);
    assert!(out.contains("found-in-note: 0"), "nothing found without a search: {out}");
}

#[test]
fn highlights_are_on_yellow_and_comments_dim_their_signs_hidden_away_from_the_cursor() {
    let (_vault, out) = probe_notes(&[("c.md", "T\n\na ==hot== b %%aside #t%% c\n\nend")], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert_eq!(out.lines().find_map(|l| l.strip_prefix("editor-shaded: ")), Some("hot"), "the signs off the yellow: {out}");
    let editor = out.lines().find_map(|l| l.strip_prefix("editor: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(editor.contains("[hidden]==|[plain]hot|[hidden]=="), "its font and colour kept: {editor}");
    assert!(editor.contains("[hidden]%%|[comment]aside #t|[hidden]%%"), "dim, the tag not styled: {editor}");
}

#[test]
fn a_footnote_shows_its_label_raised_and_a_click_goes_to_its_definition_and_back() {
    let note = [("f.md", "Text[^1] here.\n\nmiddle\n\n[^1]: The note.\n\nend")];
    let (_vault, out) = probe_notes(&note, &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let editor = out.lines().find_map(|l| l.strip_prefix("editor: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(editor.contains("[hidden][^|[footnote]1|[hidden]]"), "{editor}");
    assert!(editor.contains("[footnote-label][^1]:"), "{editor}");
    let (_vault, out) = probe_notes(&note, &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "6")]);
    assert!(out.contains("cursor: 29\n"), "after the definition's [^1]: {out}");
    let (_vault, out) = probe_notes(&note, &[("KNOTES_PROBE_OPEN", "f.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "25")]);
    assert!(out.contains("cursor: 8\n"), "back after the reference: {out}");
}

/// The embedded notes' lines (`embed: path x y w h | text`) and their boxes (`x y w h`).
fn embeds(out: &str) -> (Vec<String>, Vec<Vec<f64>>) {
    let shown = out.lines().filter_map(|l| l.strip_prefix("embed: ")).map(str::to_owned).collect();
    let boxes = out
        .lines()
        .filter_map(|l| l.strip_prefix("decoration: embed"))
        .map(|l| l.trim_start_matches("-below").split_whitespace().filter_map(|n| n.parse().ok()).collect())
        .collect();
    (shown, boxes)
}

#[test]
fn a_note_embedded_alone_on_its_line_shows_styled_in_a_box_and_its_line_hides() {
    let notes = [("a.md", "Top\n\n![[b]]\n\nMid\n\n![[b#Part]]\n\n![[a]]\n\nend"), ("b.md", "---\ntags: [x]\n---\n# B\n\nIntro **bold**.\n\n## Part\n\nPart text.\n")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let (shown, boxes) = embeds(&out);
    assert_eq!(shown.len(), 2, "itself not embedded: {out}");
    assert!(shown[0].starts_with("b.md ") && shown[0].ends_with("| # B⏎⏎Intro **bold**.⏎⏎## Part⏎⏎Part text."), "no frontmatter: {}", shown[0]);
    assert!(shown[1].ends_with("| ## Part⏎⏎Part text."), "the section alone: {}", shown[1]);
    let editors: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("embed-editor: ")).collect();
    assert!(editors[0].starts_with("[hidden]# |[heading1]B") && editors[0].contains("[strong]bold"), "its styles, no markup shown: {}", editors[0]);
    let editor = out.lines().find_map(|l| l.strip_prefix("editor: ")).unwrap();
    assert!(editor.contains("[embed]![[b]]") && editor.contains("[embed]![[b#Part]]") && editor.contains("[hidden]![[|[wikilink]a|"), "itself stays a link: {editor}");
    // Each view lies inside its box.
    let frame: Vec<f64> = shown[0].split(" | ").next().unwrap().split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
    assert_eq!(boxes.len(), 2, "{out}");
    assert!(frame[0] > boxes[0][0] && frame[1] > boxes[0][1] && frame[1] + frame[3] < boxes[0][1] + boxes[0][3], "{frame:?} in {:?}", boxes[0]);
    // The cursor on the embed's line: its source shows, the box below it.
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "7")]);
    assert!(out.contains("decoration: embed-below"), "{out}");
    let editor = out.lines().find_map(|l| l.strip_prefix("editor: ")).unwrap();
    assert!(editor.contains("![[") && !editor.contains("[embed]![[b]]"), "{editor}");
}

#[test]
fn an_embedded_note_is_edited_in_place_and_the_edit_goes_back_into_its_note() {
    let notes = [("a.md", "Top\n\n![[b#Part]]\n\nend"), ("b.md", "# B\n\n## Part\n\nPart text.\n\n## Next\n\nN.\n")];
    let height = |out: &str| embeds(out).0[0].split(" | ").next().unwrap().split_whitespace().nth(4).unwrap().parse::<f64>().unwrap();
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let before = height(&out);
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_EMBED_TYPE", " More.\n\nAnother line.")]);
    assert_eq!(std::fs::read_to_string(vault.path().join("b.md")).unwrap(), "# B\n\n## Part\n\nPart text. More.\n\nAnother line.\n\n## Next\n\nN.\n", "in its section's place: {out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "Top\n\n![[b#Part]]\n\nend", "the note around it untouched");
    assert!(height(&out) > before, "its box grew: {before} → {}", height(&out));
    assert!(out.contains("text: Top⏎⏎![[b#Part]]⏎⏎end\n"), "{out}");
}

#[test]
fn typing_an_embed_key_by_key_shows_it_without_a_crash() {
    let notes = [("a.md", "Top\n\nend"), ("b.md", "# B\n\ntext\n")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "\n![[b]]"), ("KNOTES_PROBE_TYPE_CHARS", "1")]);
    assert!(out.contains("decoration: embed-below"), "the box below the line being typed: {out}");
    assert_eq!(embeds(&out).0.len(), 1, "{out}");
}

#[test]
fn an_embed_s_button_at_its_top_right_opens_its_note_at_its_part() {
    // Long enough that its section can come to the top.
    let long = "A paragraph.\n\n".repeat(80);
    // A section with more after it: one at a note's end cannot come to the top (the view is as
    // tall as its text, the path bar's room below it).
    let notes = [("a.md", "Top\n\n![[b#Part]]\n\nend".to_owned()), ("b.md", format!("# B\n\n{long}## Part\n\nPart text.\n\n## After\n\n{long}"))];
    let notes: Vec<(&str, &str)> = notes.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let (shown, boxes) = embeds(&out);
    let button: Vec<f64> = out.lines().find_map(|l| l.strip_prefix("embed-open: ")).unwrap_or_else(|| panic!("{out}")).split_whitespace().filter_map(|n| n.parse().ok()).collect();
    let text: Vec<f64> = shown[0].split(" | ").next().unwrap().split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
    let b = &boxes[0];
    assert!(button[0] + button[2] <= b[0] + b[2] && button[0] > b[0] + b[2] - 40.0 && button[1] >= b[1] && button[1] < b[1] + 20.0, "top right: {button:?} in {b:?}");
    assert!(text[0] + text[2] <= button[0], "the text keeps clear of it: {text:?} {button:?}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_HOVER_EMBED", "0")]);
    assert!(out.contains("hover-embed-open: hand\n") && out.contains("hover-embed-text: text\n"), "a hand over the button, not over the note's text: {out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_EMBED_OPEN", "0")]);
    assert!(out.contains("note: b.md |"), "{out}");
    assert!(out.lines().any(|l| l.starts_with("note-top: ## Part")), "at its section: {out}");
}

#[test]
fn a_link_in_an_embedded_note_goes_where_it_goes_from_that_note() {
    let notes = [("a.md", "Top\n\n![[sub/b]]\n\nend"), ("sub/b.md", "See [c](c.md) here."), ("sub/c.md", "the right c")];
    // Character 6 of the embedded text is in the link's text.
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_EMBED_CLICK", "6")]);
    assert!(out.contains("note: sub/c.md |"), "from sub/b.md, not from a.md: {out}");
    assert!(!vault.path().join("c.md").exists(), "no note made where a.md would have it");
}

#[test]
fn links_and_tags_complete_in_an_embedded_note_too() {
    let notes = [("a.md", "Top\n\n![[sub/b]]\n\nend"), ("sub/b.md", "Body."), ("sub/target.md", "t"), ("c.md", "#cooking")];
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")];
    let (_vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_EMBED_TYPE", " [[targ")]);
    assert!(completion(&out).starts_with("▸target — sub"), "{out}");
    let (vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_EMBED_TYPE", " [[targ"), ("KNOTES_PROBE_EMBED_KEYS", "return")]);
    assert_eq!(std::fs::read_to_string(vault.path().join("sub/b.md")).unwrap(), "Body. [[target]]", "the link as from sub/b.md: {out}");
    let (_vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_EMBED_TYPE", " #coo")]);
    assert_eq!(completion(&out), "▸#cooking — 1", "{out}");
}

#[test]
fn cmd_f_finds_in_embedded_notes_too_in_their_place() {
    let notes = [("a.md", "radar top\n\n![[b]]\n\nradar end"), ("b.md", "inner radar here")];
    let find = |steps: &str| probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_FIND", "radar"), ("KNOTES_PROBE_FIND_STEPS", steps)]).1;
    let out = find("0");
    assert!(out.contains("find: 1 з 3\n"), "the embedded one counted: {out}");
    let out = find("1");
    assert!(out.contains("find: 2 з 3\n") && out.contains("embed-selection: 6 5\n"), "the second, in the embedded note: {out}");
    let out = find("2");
    assert!(out.contains("find: 3 з 3\n") && out.contains("selection: 19 5\n"), "then after it: {out}");
}

#[test]
fn inline_code_has_a_background_and_its_backticks_do_not() {
    let (_vault, out) = probe_notes(&[("c.md", "T\n\nrun `cargo test` now\n\nend")], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert_eq!(out.lines().find_map(|l| l.strip_prefix("editor-shaded: ")), Some("cargo test"), "{out}");
}

#[test]
fn typing_a_tag_offers_the_vault_s_tags_and_return_inserts_one_with_a_space_after_it() {
    let notes = [("b.md", "#project/radar #radar"), ("c.md", "#radar #cooking"), ("a.md", "")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "Plan #ra")]);
    assert_eq!(completion(&out), "▸#radar — 2 | #project/radar — 1", "the start first, the most used first: {out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "Plan #ra"), ("KNOTES_PROBE_KEYS", "down,return")]);
    assert!(out.contains("note: a.md | Plan #project/radar "), "{out}");
    assert!(out.contains("cursor: 20"), "after the space: {out}");
    assert_eq!(completion(&out), "none");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "`code #ra")]);
    assert_eq!(completion(&out), "none", "not in code");
}

#[test]
fn in_the_frontmatter_the_tags_list_offers_tags_without_a_hash() {
    let notes = [("b.md", "#radar"), ("a.md", "")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "---\ntags: [ra"), ("KNOTES_PROBE_KEYS", "return")]);
    assert!(out.contains("note: a.md | ---⏎tags: [radar"), "no hash, no space: {out}");
}

#[test]
fn in_a_space_its_tags_are_offered_first() {
    let vault = spaced_vault();
    std::fs::write(vault.path().join("Home/d.md"), "#rabbit").unwrap();
    std::fs::write(vault.path().join("top.md"), "top #rabbit").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_OPEN", "Work/a.md"), ("KNOTES_PROBE_TYPE", " #ra")]);
    assert_eq!(completion(&out), "▸#radar — 1 | #rabbit — 2", "the space's first, though less used: {out}");
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
    assert_eq!(decorations(&out, "external").len(), 1, "in the cursor's line too, the cursor outside the link: {out}");
    let (_vault, out) = probe_notes(&[("a.md", note), ("b.md", "b")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "10")]);
    assert!(decorations(&out, "external").is_empty(), "the source of the link the cursor is in shows: {out}");
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
    assert!((widths[1] - 282.0).abs() < 2.0, "the list as wide as before the sidebar went: {widths:?}");
}

/// Zen mode folds all but the note away; again, and each column is as it was (the panel
/// folded before it stays folded).
#[test]
fn zen_mode_leaves_the_note_alone_and_gives_the_columns_back() {
    let folded = |action: &str| {
        let (_vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", action), ("KNOTES_PROBE_DELAY_MS", "2500")]);
        let widths: Vec<f64> = out.lines().find_map(|l| l.strip_prefix("widths: ")).unwrap().split(' ').map(|w| w.parse().unwrap()).collect();
        let tabs_hidden = out.lines().any(|l| l.starts_with("tabs: ") && l.ends_with(" (hidden)"));
        (format!("{}{}", out.lines().find_map(|l| l.strip_prefix("folded: ")).unwrap(), if tabs_hidden { ", no tabs" } else { "" }), widths)
    };
    let (before, widths_before) = folded("none");
    assert_eq!(folded("zen").0, "folded folded shown folded, no toolbar, no tabs");
    // A note opened in zen mode brings no column back, and only its name is over it.
    let (_vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "zen"), ("KNOTES_PROBE_OPEN", "a.md")]);
    assert!(out.contains("folded: folded folded shown folded, no toolbar"), "{out}");
    assert!(out.contains("note-title: a\n") && out.contains("note-date: hidden") && out.contains("path-bar: hidden"), "{out}");
    let off: f64 = out.lines().find_map(|l| l.strip_prefix("note-title-off-middle: ")).unwrap().parse().unwrap();
    assert!(off <= 1.0, "the name in the middle of the window: {out}");
    let (after, widths) = folded("zen-twice");
    assert_eq!(after, before);
    assert!((widths[1] - widths_before[1]).abs() < 2.0, "the list as wide as before: {widths:?} {widths_before:?}");
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

#[test]
fn a_double_click_on_an_image_previews_its_file() {
    let (vault, out) = probe_images(
        &[("a.md", "Before\n![[pic.png]]\nAfter")],
        &[("att/pic.png", PIC)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_QUICK_LOOK", "7")],
    );
    let file = out.lines().find_map(|l| l.strip_prefix("quick-look: ")).unwrap_or_else(|| panic!("{out}"));
    assert_eq!(std::path::Path::new(file), vault.path().canonicalize().unwrap().join("att/pic.png"), "{out}");
}

#[test]
fn only_images_in_view_are_decoded() {
    let text: String = (0..30).map(|i| format!("Line {i}\n![[wide{i}.png]]\n")).collect();
    let names: Vec<String> = (0..30).map(|i| format!("wide{i}.png")).collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), WIDE)).collect();
    let (_vault, out) = probe_images(&[("a.md", &text)], &files, &[("KNOTES_PROBE_OPEN", "a.md")]);
    let decoded: usize = out.lines().find_map(|l| l.strip_prefix("images-decoded: ")).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("{out}"));
    assert!(decoded > 0 && decoded < 15, "{decoded} of 30 decoded: {out}");
}

#[test]
fn opening_another_note_lets_the_last_one_s_images_go() {
    let (_vault, out) = probe_images(
        &[("a.md", "![[one.png]]\n![[two.png]]\nend"), ("b.md", "![[three.png]]\nend")],
        &[("one.png", PIC), ("two.png", PIC), ("three.png", WIDE)],
        &[("KNOTES_PROBE_OPEN", "a.md,b.md")],
    );
    assert!(out.contains("note: b.md"), "{out}");
    assert!(out.contains("images-kept: 1\n"), "only the open note's image is kept: {out}");
}

#[test]
fn an_image_rewritten_on_disk_shows_anew() {
    let wide = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/wide-1600x400.png");
    let (_vault, out) = probe_images(
        &[("a.md", "Before\n![[pic.png]]\nAfter")],
        &[("pic.png", PIC)],
        &[
            ("KNOTES_PROBE_OPEN", "a.md"),
            ("KNOTES_PROBE_CURSOR", "end"),
            ("KNOTES_PROBE_REWRITE_IMAGE", &format!("pic.png={wide}")),
            ("KNOTES_PROBE_DELAY_MS", "2500"),
        ],
    );
    let shown = images(&out);
    assert_eq!(shown.len(), 1, "{out}");
    assert!(shown[0].2 > 300.0 && (shown[0].2 / shown[0].3 - 4.0).abs() < 0.05, "the new, wide picture: {out}");
}

#[test]
fn images_far_out_of_view_are_let_go() {
    let text: String = (0..30).map(|i| format!("Line {i}\n![[wide{i}.png]]\n")).collect();
    let names: Vec<String> = (0..30).map(|i| format!("wide{i}.png")).collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), WIDE)).collect();
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NOTE_SCROLL", "end-later"), ("KNOTES_PROBE_DELAY_MS", "1000")];
    let (_vault, out) = probe_images(&[("a.md", &text)], &files, &env);
    let ready: Vec<&str> = out.lines().find_map(|l| l.strip_prefix("images-ready: ")).unwrap_or_else(|| panic!("{out}")).split(' ').collect();
    assert!(ready.contains(&"wide29.png"), "the images in view are decoded: {ready:?}");
    assert!(!ready.contains(&"wide0.png") && !ready.contains(&"wide1.png"), "those at the top are let go: {ready:?}");
}

/// `decoration: image-handle x y w h` lines.
fn image_handles(out: &str) -> Vec<(f64, f64, f64, f64)> {
    out.lines()
        .filter_map(|l| l.strip_prefix("decoration: image-handle "))
        .map(|l| {
            let p: Vec<f64> = l.split(' ').map(|v| v.parse().unwrap()).collect();
            (p[0], p[1], p[2], p[3])
        })
        .collect()
}

#[test]
fn the_pointer_over_an_image_shows_its_resize_handle_on_the_right_edge() {
    let notes = [("a.md", "Before\n![[pic.png]]\nAfter")];
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")];
    let (_vault, out) = probe_images(&notes, &[("pic.png", PIC)], &env);
    assert!(image_handles(&out).is_empty(), "no handle without the pointer: {out}");
    let (x, y, w, h, _) = images(&out)[0].clone();
    let over = format!("{},{}", x + w / 2.0, y + h / 2.0);
    let (_vault, out) = probe_images(&notes, &[("pic.png", PIC)], &[env[0], env[1], ("KNOTES_PROBE_IMAGE_HOVER", &over)]);
    let handles = image_handles(&out);
    assert_eq!(handles.len(), 1, "{out}");
    let (hx, hy, hw, hh) = handles[0];
    assert!(hx > x + w - 12.0 && hx + hw <= x + w, "at the image's right edge, inside it: {handles:?} {x} {w}");
    assert!((hy + hh / 2.0 - (y + h / 2.0)).abs() < 1.0, "halfway down: {handles:?}");
    let away = format!("{},{}", x + w / 2.0, y + h + 40.0);
    let (_vault, out) = probe_images(&notes, &[("pic.png", PIC)], &[env[0], env[1], ("KNOTES_PROBE_IMAGE_HOVER", &format!("{over};{away}"))]);
    assert!(image_handles(&out).is_empty(), "gone with the pointer: {out}");
}

#[test]
fn spelling_marks_stay_off_an_image_s_hidden_source() {
    let (_vault, out) = probe_images(
        &[("a.md", "Wrods\n![[Sonoma pic.png]]\nend")],
        &[("Sonoma pic.png", PIC)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_SPELLING", "0,5;9,6")],
    );
    assert!(out.contains("spelling: kept dropped"), "{out}");
}

#[test]
fn an_image_on_the_last_line_shows_right_below_its_source() {
    let notes = [("a.md", "Before\n\n![[wide.png]]\n")];
    let shown = |cursor: &str| {
        let (_vault, out) = probe_images(&notes, &[("wide.png", WIDE)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", cursor)]);
        images(&out)[0].clone()
    };
    let (away, near) = (shown("0"), shown("10"));
    assert_eq!(near.4, "preview");
    assert!(near.1 - away.1 < 30.0, "below the source's line, not the empty line after it: {away:?} {near:?}");
}

const TABLE: &str = "Before\n\n| Name | Age |\n|:---|---:|\n| Ann | 3 |\n| Bartholomew | 42 |\n\nAfter";

/// `range-x: x0:x1 …` of `KNOTES_PROBE_RANGE_X`, as pairs.
fn range_xs(out: &str) -> Vec<(f64, f64)> {
    out.lines()
        .find_map(|l| l.strip_prefix("range-x: "))
        .unwrap_or_else(|| panic!("{out}"))
        .split(' ')
        .map(|p| {
            let (a, b) = p.split_once(':').unwrap();
            (a.parse().unwrap(), b.parse().unwrap())
        })
        .collect()
}

#[test]
fn a_table_s_cells_line_up_in_columns_with_their_alignment() {
    // Name, Ann, Bartholomew (left aligned); Age, 3, 42 (right aligned).
    let ranges = "10,4;37,3;49,11;17,3;43,1;63,2";
    let (_vault, out) = probe_notes(&[("a.md", TABLE)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_RANGE_X", ranges)]);
    let x = range_xs(&out);
    assert!((x[0].0 - x[1].0).abs() < 0.6 && (x[0].0 - x[2].0).abs() < 0.6, "the first column starts its cells together: {x:?}");
    assert!((x[3].1 - x[4].1).abs() < 0.6 && (x[3].1 - x[5].1).abs() < 0.6, "the second column ends its cells together: {x:?}");
    assert!(x[3].0 > x[2].1 + 10.0, "the second column comes after the widest first cell: {x:?}");
    assert!(out.contains("[table]|"), "the pipes are hidden: {out}");
}

#[test]
fn a_table_s_rows_draw_their_grid_and_the_delimiter_row_folds_away() {
    let (_vault, out) = probe_notes(&[("a.md", TABLE)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "0")]);
    let rows: Vec<(f64, f64, f64, f64, &str)> = out
        .lines()
        .filter_map(|l| l.strip_prefix("decoration: table-row "))
        .map(|l| {
            let p: Vec<&str> = l.split(' ').collect();
            (p[0].parse().unwrap(), p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap(), p[4])
        })
        .collect();
    assert_eq!(rows.iter().map(|r| r.4).collect::<Vec<_>>(), ["header", "body", "body"], "{out}");
    assert!(rows.iter().all(|r| (r.2 - rows[0].2).abs() < 0.5 && (r.0 - rows[0].0).abs() < 0.5), "one width: {rows:?}");
    assert!((rows[1].1 - (rows[0].1 + rows[0].3)).abs() < 1.5, "the first body row right under the header: {rows:?}");
}

#[test]
fn a_table_wider_than_the_column_keeps_each_row_on_one_line() {
    let wide = "| Name | A rather long heading for this column | Another long heading here |\n|---|---|---|\n| a | b | c |\n";
    let (_vault, out) = probe_notes(&[("a.md", wide)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let rows: Vec<(f64, f64)> = out
        .lines()
        .filter_map(|l| l.strip_prefix("decoration: table-row "))
        .map(|l| {
            let p: Vec<f64> = l.split(' ').take(4).map(|v| v.parse().unwrap()).collect();
            (p[2], p[3])
        })
        .collect();
    let text_width: f64 = out.lines().find_map(|l| l.strip_prefix("text-layout: ")).and_then(|l| l.split(' ').nth(1)).and_then(|w| w.strip_prefix("width=")).unwrap().parse().unwrap();
    assert!(rows[0].0 > text_width, "the table is wider than the column: {rows:?} {text_width}");
    assert!((rows[0].1 - rows[1].1).abs() < 0.5, "the header is one line, as a short row: {rows:?}");
}

/// The note's whole text (`text:` line), line breaks as ⏎.
fn note_text(out: &str) -> &str {
    out.lines().find_map(|l| l.strip_prefix("text: ")).unwrap_or_else(|| panic!("{out}"))
}

fn cursor(out: &str) -> usize {
    out.lines().find_map(|l| l.strip_prefix("cursor: ")).and_then(|c| c.parse().ok()).unwrap_or_else(|| panic!("{out}"))
}

fn table_probe(env: &[(&str, &str)]) -> String {
    let mut all = vec![("KNOTES_PROBE_OPEN", "a.md")];
    all.extend_from_slice(env);
    probe_notes(&[("a.md", TABLE)], &all).1
}

#[test]
fn typing_in_a_cell_edits_it_and_a_pipe_is_escaped() {
    let out = table_probe(&[("KNOTES_PROBE_TYPE", "x|"), ("KNOTES_PROBE_RANGE", "40,0")]);
    assert!(note_text(&out).contains("| Annx\\| | 3 |"), "{out}");
}

#[test]
fn the_table_s_hidden_pipes_and_spaces_cannot_be_deleted() {
    let out = table_probe(&[("KNOTES_PROBE_TYPE", ""), ("KNOTES_PROBE_RANGE", "36,1")]);
    assert!(note_text(&out).contains("⏎| Ann | 3 |⏎"), "{out}");
}

#[test]
fn typing_between_cells_goes_into_the_nearest_one() {
    let out = table_probe(&[("KNOTES_PROBE_TYPE", "x"), ("KNOTES_PROBE_RANGE", "41,0")]);
    assert!(note_text(&out).contains("| Annx | 3 |"), "{out}");
}

#[test]
fn tab_return_and_arrows_move_between_cells() {
    // An empty insertion puts the cursor there before the keys (KNOTES_PROBE_CURSOR comes after).
    let at = |cursor_at: &str, keys: &str| {
        let range = format!("{cursor_at},0");
        cursor(&table_probe(&[("KNOTES_PROBE_TYPE", ""), ("KNOTES_PROBE_RANGE", &range), ("KNOTES_PROBE_KEYS", keys)]))
    };
    assert_eq!(at("40", "tab"), 44, "Tab: to the end of the next cell");
    assert_eq!(at("44", "backtab"), 40, "Shift+Tab: back");
    assert_eq!(at("40", "return"), 60, "Return: the same column of the next row");
    assert_eq!(at("14", "down"), 40, "down from the header skips the delimiter row");
    assert_eq!(at("40", "up"), 14);
    assert_eq!(at("40", "right"), 43, "right at a cell's end: the next cell's start");
    assert_eq!(at("43", "left"), 40);
}

#[test]
fn tab_in_the_last_cell_adds_a_row() {
    let out = table_probe(&[("KNOTES_PROBE_TYPE", ""), ("KNOTES_PROBE_RANGE", "65,0"), ("KNOTES_PROBE_KEYS", "tab")]);
    assert!(note_text(&out).contains("| Bartholomew | 42 |⏎|  |  |⏎⏎After"), "{out}");
    assert_eq!(cursor(&out), 74, "in the new row's first cell: {out}");
}

#[test]
fn a_cursor_between_cells_moves_into_the_nearest_one() {
    assert_eq!(cursor(&table_probe(&[("KNOTES_PROBE_CURSOR", "41")])), 40, "on the hidden pipe after Ann");
    assert_eq!(cursor(&table_probe(&[("KNOTES_PROBE_CURSOR", "28")])), 20, "on the folded delimiter row: the header's end");
}

#[test]
fn the_pointer_over_a_table_shows_its_row_and_column_handles_and_add_bars() {
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0")]);
    assert!(decorations(&out, "table-row-handle").is_empty() && decorations(&out, "table-add-row").is_empty(), "nothing without the pointer: {out}");
    let rows = decorations(&out, "table-row");
    let ann = rows[1];
    let over = format!("{},{}", ann.0 + ann.2 * 0.8, ann.1 + ann.3 / 2.0);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over)]);
    let handle = decorations(&out, "table-row-handle");
    assert_eq!(handle.len(), 1, "{out}");
    assert!(handle[0].0 + handle[0].2 <= ann.0 && (handle[0].1 + handle[0].3 / 2.0 - (ann.1 + ann.3 / 2.0)).abs() < 1.0, "left of the row, halfway down: {handle:?} {ann:?}");
    let column = decorations(&out, "table-column-handle");
    assert_eq!(column.len(), 1, "{out}");
    assert!(column[0].1 + column[0].3 <= rows[0].1 + 1.0 && column[0].0 > ann.0 + ann.2 / 2.0, "over the second column's header: {column:?}");
    let add_row = decorations(&out, "table-add-row");
    let last = rows[2];
    assert!(add_row.len() == 1 && add_row[0].1 >= last.1 + last.3 - 1.0, "under the table: {add_row:?}");
    assert!(!decorations(&out, "table-add-column").is_empty(), "{out}");
    let away = format!("{over};{},{}", ann.0 + ann.2 / 2.0, last.1 + last.3 + 120.0);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &away)]);
    assert!(decorations(&out, "table-row-handle").is_empty(), "gone with the pointer: {out}");
}

/// Centre of a decoration's rect, as a probe point.
fn centre(r: (f64, f64, f64, f64)) -> String {
    format!("{},{}", r.0 + r.2 / 2.0, r.1 + r.3 / 2.0)
}

/// The table's controls with the pointer over row `row`, column `column` (from the grid's
/// layout), and the hover point to give first.
fn table_controls_over(row: usize, column_fraction: f64) -> (String, String) {
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0")]);
    let rows = decorations(&out, "table-row");
    let r = rows[row];
    let over = format!("{},{}", r.0 + r.2 * column_fraction, r.1 + r.3 / 2.0);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over)]);
    (over, out)
}

#[test]
fn the_add_bars_add_a_row_and_a_column() {
    let (over, out) = table_controls_over(1, 0.8);
    let add_row = centre(decorations(&out, "table-add-row")[0]);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &add_row)]);
    assert!(note_text(&out).contains("| Bartholomew | 42 |⏎|  |  |⏎"), "{out}");
    let (over, out) = table_controls_over(1, 0.8);
    let add_column = centre(decorations(&out, "table-add-column")[0]);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &add_column)]);
    assert!(note_text(&out).contains("| Name | Age |  |⏎| :--- | ---: | --- |⏎| Ann | 3 |  |"), "{out}");
}

#[test]
fn a_row_handle_opens_the_row_menu_and_its_items_act() {
    let (over, out) = table_controls_over(1, 0.8);
    let handle = centre(decorations(&out, "table-row-handle")[0]);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &handle)]);
    assert!(out.contains("table-menu: Вставити рядок вище | Вставити рядок нижче | Видалити рядок"), "{out}");
    let choose = format!("{handle}:2");
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &choose)]);
    assert!(note_text(&out).contains("| :--- | ---: |⏎| Bartholomew | 42 |⏎⏎After"), "Ann's row gone: {out}");
}

#[test]
fn a_column_handle_opens_the_column_menu_with_alignment_and_sorting() {
    let (over, out) = table_controls_over(1, 0.8);
    let handle = centre(decorations(&out, "table-column-handle")[0]);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &handle)]);
    let menu = out.lines().find_map(|l| l.strip_prefix("table-menu: ")).unwrap_or_else(|| panic!("{out}"));
    let items: Vec<&str> = menu.split(" | ").collect();
    assert_eq!(items, ["Вставити колонку ліворуч", "Вставити колонку праворуч", "Видалити колонку", "Вирівняти ліворуч", "Вирівняти по центру", "Вирівняти праворуч", "Сортувати за зростанням", "Сортувати за спаданням"]);
    let choose = format!("{handle}:7");
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &choose)]);
    assert!(note_text(&out).contains("| Bartholomew | 42 |⏎| Ann | 3 |"), "sorted by Age, descending: {out}");
}

#[test]
fn dragging_a_row_handle_moves_the_row() {
    let (over, out) = table_controls_over(2, 0.3);
    let handle = decorations(&out, "table-row-handle")[0];
    let rows = decorations(&out, "table-row");
    // From Bartholomew's handle up to the top of Ann's row.
    let drag = format!("{}>{},{}", centre(handle), handle.0 + 5.0, rows[1].1 + 3.0);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_PRESS", &drag)]);
    assert!(note_text(&out).contains("| Bartholomew | 42 |⏎| Ann | 3 |⏎"), "{out}");
}

const WIDE_TABLE: &str = "| First column here | Second column heading text | Third column heading text | Fourth |\n|---|---|---|---|\n| a | b | c | d |\n\nAfter";

#[test]
fn a_wide_table_scrolls_sideways_smoothly_and_follows_the_cursor() {
    let probe = |env: &[(&str, &str)]| {
        let mut all = vec![("KNOTES_PROBE_OPEN", "a.md")];
        all.extend_from_slice(env);
        probe_notes(&[("a.md", WIDE_TABLE)], &all).1
    };
    // "First column here" at 2..19, "Second column heading text" at 22..48, "Fourth" at 79..85.
    let ranges = "2,17;22,26;79,6";
    let out = probe(&[("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_RANGE_X", ranges)]);
    let before = range_xs(&out);
    let row = decorations(&out, "table-row")[0];
    let over = format!("{},{}", row.0 + 40.0, row.1 + row.3 / 2.0);
    let scrolled = |dx: &str, extra: &str| {
        let scroll = format!("{over};{dx}{extra}");
        probe(&[("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_SCROLL", &scroll), ("KNOTES_PROBE_RANGE_X", ranges)])
    };
    let after = range_xs(&scrolled("-60", ""));
    assert!((after[1].0 - (before[1].0 - 60.0)).abs() < 1.0, "the second column 60 points left: {before:?} {after:?}");
    assert!((after[0].1 - (before[0].1 - 60.0)).abs() < 1.0, "the first column's end too: {after:?}");
    assert!(after[0].0 >= row.0 - 0.5 && after[0].0 < row.0 + 10.0, "its start cut at the table's left edge: {after:?}");
    // As far as it goes: the last column's text ends inside the text column.
    let out = scrolled("-10000", "");
    let end = range_xs(&out)[2].1;
    let text: Vec<f64> = out.lines().find_map(|l| l.strip_prefix("text-layout: ")).unwrap().split(' ').skip(1).map(|p| p.split('=').nth(1).unwrap().parse().unwrap()).collect();
    let column_right = text[1] + text[0] - 5.0;
    assert!(end < column_right && end > column_right - 30.0, "scrolled to the table's right edge: {end} {column_right}");
    // Back with the cursor put in the first column.
    let back = range_xs(&scrolled("-60", ";cursor=5"));
    assert!((back[0].0 - before[0].0).abs() < 0.5, "the cursor's column shows again: {before:?} {back:?}");
}

#[test]
fn a_right_click_in_a_cell_offers_the_table_s_row_and_column_actions() {
    // In Ann's cell (37..40).
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_CONTEXT", "38")]);
    let menu = out.lines().find_map(|l| l.strip_prefix("table-context: ")).unwrap_or_else(|| panic!("{out}"));
    let items: Vec<&str> = menu.split(" | ").collect();
    assert_eq!(&items[..6], ["Вставити рядок вище", "Вставити рядок нижче", "Видалити рядок", "Вставити колонку ліворуч", "Вставити колонку праворуч", "Видалити колонку"]);
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_CONTEXT", "38:2")]);
    assert!(note_text(&out).contains("| :--- | ---: |⏎| Bartholomew | 42 |⏎"), "Ann's row deleted: {out}");
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_CONTEXT", "38:5")]);
    assert!(note_text(&out).contains("| Age |⏎| ---: |⏎| 3 |⏎| 42 |"), "the first column deleted: {out}");
    let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_CONTEXT", "70")]);
    assert!(out.contains("table-context: none"), "outside a table: {out}");
}

#[test]
fn a_right_click_on_a_handle_or_a_cell_s_padding_offers_deleting() {
    let (over, out) = table_controls_over(1, 0.8);
    let row_handle = centre(decorations(&out, "table-row-handle")[0]);
    let column_handle = centre(decorations(&out, "table-column-handle")[0]);
    let context = |at: &str| {
        let out = table_probe(&[("KNOTES_PROBE_CURSOR", "0"), ("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_CONTEXT_AT", at)]);
        out.lines().find_map(|l| l.strip_prefix("table-context: ")).map(str::to_owned).unwrap_or_else(|| panic!("{out}"))
    };
    assert_eq!(context(&row_handle), "Вставити рядок вище | Вставити рядок нижче | Видалити рядок");
    assert!(context(&column_handle).contains("Видалити колонку"));
    // On the padding just inside the row's left border, away from any text.
    let rows = decorations(&out, "table-row");
    let padding = format!("{},{}", rows[1].0 + 3.0, rows[1].1 + rows[1].3 / 2.0);
    assert!(context(&padding).contains("Видалити рядок"), "the nearest cell's actions");
}

#[test]
fn rows_out_of_view_catch_up_with_a_sideways_scroll() {
    let mut text = String::from("| First column here | Second column heading text | Third column heading text | Fourth |\n|---|---|---|---|\n");
    for i in 0..80 {
        text += &format!("| row {i} | b | c | d |\n");
    }
    text += "\nAfter";
    let probe = |env: &[(&str, &str)]| {
        let mut all = vec![("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "0")];
        all.extend_from_slice(env);
        probe_notes(&[("a.md", &text)], &all).1
    };
    let out = probe(&[]);
    let row = decorations(&out, "table-row")[0];
    let over = format!("{},{}", row.0 + 40.0, row.1 + row.3 / 2.0);
    let scroll = format!("{over};-60");
    let out = probe(&[("KNOTES_PROBE_TABLE_HOVER", &over), ("KNOTES_PROBE_TABLE_SCROLL", &scroll), ("KNOTES_PROBE_NOTE_SCROLL", "end-later"), ("KNOTES_PROBE_DELAY_MS", "1000")]);
    let rows = decorations(&out, "table-row");
    assert!(rows.len() > 5, "{out}");
    assert!(rows.iter().all(|r| (r.0 - (row.0 - 60.0)).abs() < 0.5), "every row in view at the end scrolled too: {rows:?} (from {})", row.0);
}

const PDF: &[u8] = include_bytes!("fixtures/three-pages.pdf");

#[test]
fn a_pdf_embed_shows_its_page_and_how_many_there_are() {
    let (_vault, out) = probe_images(
        &[("a.md", "Before\n![[doc.pdf#page=2]]\n![[doc.pdf|120]]\nAfter")],
        &[("doc.pdf", PDF)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")],
    );
    let shown = images(&out);
    assert_eq!(shown.iter().map(|i| (i.2, i.3)).collect::<Vec<_>>(), [(200.0, 100.0), (120.0, 60.0)], "{out}");
    assert!(out.contains("pdf-caption: сторінка 2 з 3") && out.contains("pdf-caption: сторінка 1 з 3"), "{out}");
    let captions = decorations(&out, "pdf-caption");
    assert!(captions[0].1 >= shown[0].1 + shown[0].3, "the caption under the page: {captions:?} {shown:?}");
    assert!(shown[1].1 > captions[0].1 + captions[0].3, "the next preview below the caption: {shown:?} {captions:?}");
}

#[test]
fn a_click_on_a_pdf_preview_opens_the_pdf_in_place_of_the_note_and_back_returns() {
    let notes = [("a.md", "Before\n![[doc.pdf#page=2]]\nAfter")];
    // Character 7 keeps the preview's width: its middle is the preview's.
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "7")];
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &env);
    assert!(out.contains("pdf-view: doc.pdf 2/3"), "{out}");
    assert!(out.contains("note-title: doc\n"), "{out}");
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &[env[0], env[1], env[2], ("KNOTES_PROBE_AFTER_CLICK", "back")]);
    assert!(out.contains("pdf-view: hidden") && out.contains("note: a.md"), "{out}");
}

#[test]
fn a_link_to_a_pdf_opens_it_at_its_page() {
    let (_vault, out) = probe_images(
        // The cursor on another line: the link's markup hidden, a plain click follows it.
        &[("a.md", "See [[doc.pdf#page=3]] here\n\nend")],
        &[("doc.pdf", PDF)],
        &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")],
    );
    assert!(out.contains("pdf-view: doc.pdf 3/3"), "{out}");
}

#[test]
fn a_dropped_pdf_is_copied_and_embedded() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("paper.pdf"), PDF).unwrap();
    let drop = format!("{}@0", source.path().join("paper.pdf").display());
    let (vault, out) = probe_images(&[("a.md", "Text")], &[], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_DROP", &drop)]);
    assert_eq!(attachments(vault.path()), ["paper.pdf"], "{out}");
    assert!(out.contains("note: a.md | ![[paper.pdf]]⏎Text"), "{out}");
}

#[test]
fn the_search_finds_a_pdf_by_its_text_as_an_entry_of_its_own() {
    let notes = [("Docs/a.md", "a note about apples")];
    let pdfs = [("Docs/doc.pdf", PDF)];
    let (_vault, out) = probe_images(&notes, &pdfs, &[("KNOTES_PROBE_SEARCH", "content: page")]);
    assert_eq!(list_titles(&out), ["doc.pdf"], "{out}");
    let (_vault, out) = probe_images(&notes, &pdfs, &[("KNOTES_PROBE_SELECT", "f:Docs"), ("KNOTES_PROBE_SEARCH", "content: page")]);
    assert_eq!(list_titles(&out), ["doc.pdf"], "searched in its folder too: {out}");
    let (_vault, out) = probe_images(&notes, &pdfs, &[("KNOTES_PROBE_SELECT", "f:Docs")]);
    assert_eq!(list_titles(&out), ["a"], "the folder's list shows its notes only: {out}");
}

#[test]
fn dragging_a_pdf_preview_s_right_edge_resizes_it_and_opens_nothing() {
    let notes = [("a.md", "Before\n![[doc.pdf]]\nAfter")];
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")];
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &env);
    let (x, y, w, h, _) = images(&out)[0].clone();
    // From 2 points inside the right edge to 48 points past it: the centred preview's right
    // edge comes under the pointer, 200 + 2 × 48 wide.
    let drag = format!("{},{}>{},{}", x + w - 2.0, y + h / 2.0, x + w + 48.0, y + h / 2.0);
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &[env[0], env[1], ("KNOTES_PROBE_DRAG", &drag)]);
    assert!(out.contains("pdf-view: hidden"), "no viewer: {out}");
    assert!(out.contains("note: a.md | Before⏎![[doc.pdf|296]]⏎After"), "{out}");
}

/// A PDF of `pages` A4 pages, "Page N" on each.
fn pdf_of(pages: usize) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = vec![b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec()];
    let parent = 1 + 2 * pages + 1;
    let mut kids = Vec::new();
    for n in 1..=pages {
        let stream = format!("BT /F1 24 Tf 60 700 Td (Page {n}) Tj ET");
        objects.push(format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()).into_bytes());
        let contents = objects.len();
        objects.push(format!("<< /Type /Page /Parent {parent} 0 R /MediaBox [0 0 595 842] /Contents {contents} 0 R /Resources << /Font << /F1 1 0 R >> >> >>").into_bytes());
        kids.push(format!("{} 0 R", objects.len()));
    }
    objects.push(format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.join(" ")).into_bytes());
    objects.push(format!("<< /Type /Catalog /Pages {parent} 0 R >>").into_bytes());
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1, objects.len()).bytes());
    out
}

/// The pages the PDF viewer keeps drawn (`pdf-pages-kept:` line).
fn pages_kept(out: &str) -> Vec<usize> {
    out.lines().find_map(|l| l.strip_prefix("pdf-pages-kept:")).unwrap_or_else(|| panic!("{out}")).split_whitespace().map(|p| p.parse().unwrap()).collect()
}

#[test]
fn the_pdf_viewer_keeps_only_the_page_shown_and_two_either_side() {
    let long = pdf_of(30);
    let notes = [("a.md", "See [[long.pdf#page=15]] here\n\nend")];
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    let (_vault, out) = probe_images(&notes, &[("long.pdf", &long)], &env);
    assert!(out.contains("pdf-view: long.pdf 15/30"), "{out}");
    let kept = pages_kept(&out);
    assert!(kept.contains(&15) && kept.iter().all(|p| (13..=17).contains(p)), "{kept:?}");
    // Only those near the top and near the page linked to were ever drawn: before the viewer was
    // in its window, every page counted as visible, and all 30 were set to be drawn.
    let drawings: usize = out.lines().find_map(|l| l.strip_prefix("pdf-drawings: ")).unwrap_or_else(|| panic!("{out}")).parse().unwrap();
    assert!(drawings <= 10, "{drawings} pages drawn: {out}");
    let (_vault, out) = probe_images(&notes, &[("long.pdf", &long)], &[env[0], env[1], env[2], ("KNOTES_PROBE_PDF_GO", "25")]);
    let kept = pages_kept(&out);
    assert!(out.contains("pdf-view: long.pdf 25/30") && kept.contains(&25) && kept.iter().all(|p| (23..=27).contains(p)), "{kept:?} {out}");
    let (_vault, out) = probe_images(&notes, &[("long.pdf", &long)], &[env[0], env[1], env[2], ("KNOTES_PROBE_AFTER_CLICK", "back")]);
    assert!(out.contains("pdf-view: hidden") && pages_kept(&out).is_empty(), "all let go: {out}");
}

/// The text selected in the PDF viewer (`pdf-selection:` line).
fn pdf_selection(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("pdf-selection: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

/// The three-page PDF opened from a link, then `gestures` with the mouse on its pages
/// (`KNOTES_PROBE_PDF_MOUSE`: `page:x,y>page:x,y` a drag, `page:x,y*2` a double click, `^` before
/// a click with Shift; x and y as fractions of the page).
fn pdf_mouse(gestures: &str) -> String {
    pdf_mouse_with(gestures, &[])
}

/// As `pdf_mouse`, with more probe settings.
fn pdf_mouse_with(gestures: &str, more: &[(&str, &str)]) -> String {
    let mut env = vec![("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_PDF_MOUSE", gestures)];
    env.extend_from_slice(more);
    let (_vault, out) = probe_images(&[("a.md", "See [[doc.pdf]] here\n\nend")], &[("doc.pdf", PDF)], &env);
    out
}

// "Page N" on each page of the three-page PDF: x 0.21–0.56, y 0.43–0.65.

#[test]
fn dragging_over_a_pdf_page_s_text_selects_it() {
    assert_eq!(pdf_selection(&pdf_mouse("1:0.1,0.5>1:0.47,0.5")), "Page");
    assert_eq!(pdf_selection(&pdf_mouse("1:0.9,0.5>1:0.1,0.5")), "Page 1", "backwards too");
}

#[test]
fn a_selection_can_run_over_several_pdf_pages() {
    assert_eq!(pdf_selection(&pdf_mouse("1:0.1,0.5>3:0.9,0.5")), "Page 1⏎Page 2⏎Page 3");
}

#[test]
fn a_double_click_selects_a_word_and_a_triple_click_a_line_in_a_pdf() {
    assert_eq!(pdf_selection(&pdf_mouse("2:0.3,0.5*2")), "Page");
    assert_eq!(pdf_selection(&pdf_mouse("2:0.3,0.5*3")), "Page 2");
}

#[test]
fn a_click_in_a_pdf_clears_the_selection_and_shift_click_extends_it() {
    assert_eq!(pdf_selection(&pdf_mouse("1:0.1,0.5>1:0.9,0.5;1:0.3,0.5")), "");
    assert_eq!(pdf_selection(&pdf_mouse("1:0.1,0.5;^2:0.47,0.5")), "Page 1⏎Page");
}

#[test]
fn the_selected_pdf_text_is_copied_from_the_edit_menu_or_the_context_menu() {
    // Copied to a pasteboard of the probe's own, never the user's.
    let out = pdf_mouse_with("1:0.1,0.5>2:0.47,0.5", &[("KNOTES_PROBE_PDF_COPY", "1")]);
    assert!(out.contains("pdf-copied: Page 1⏎Page\n"), "{out}");
    assert!(out.contains("copy-menu: on\n") && out.contains("pdf-menu: Копіювати\n"), "{out}");
    let out = pdf_mouse_with("1:0.3,0.5", &[("KNOTES_PROBE_PDF_COPY", "1")]);
    assert!(out.contains("copy-menu: off\n") && out.contains("pdf-menu: none\n"), "nothing selected: {out}");
}

/// The pages whose text the PDF viewer keeps (`pdf-texts-kept:` line).
fn texts_kept(out: &str) -> Vec<usize> {
    out.lines().find_map(|l| l.strip_prefix("pdf-texts-kept:")).unwrap_or_else(|| panic!("{out}")).split_whitespace().map(|p| p.parse().unwrap()).collect()
}

#[test]
fn the_pdf_viewer_keeps_the_text_of_pages_in_view_and_of_pages_selected_only() {
    let long = pdf_of(30);
    let notes = [("a.md", "See [[long.pdf#page=15]] here\n\nend")];
    let probe = |gestures: &str, go: Option<&str>| {
        let mut env = vec![("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_PDF_MOUSE", gestures)];
        env.extend(go.map(|page| ("KNOTES_PROBE_PDF_GO", page)));
        probe_images(&notes, &[("long.pdf", &long)], &env).1
    };
    // "Page N" at y 0.15–0.17 of each page.
    let out = probe("15:0.05,0.16>16:0.5,0.16", Some("25"));
    assert_eq!((pdf_selection(&out), texts_kept(&out)), ("Page 15⏎Page 16".to_owned(), vec![15, 16]), "selected, out of view: {out}");
    let out = probe("15:0.3,0.5", None);
    assert_eq!(texts_kept(&out), [15], "clicked, in view: {out}");
    let out = probe("15:0.05,0.16>16:0.5,0.16;15:0.3,0.5", Some("25"));
    assert!(texts_kept(&out).is_empty(), "the selection cleared, then out of view: {out}");
}

/// A HOME whose settings show the outline in the right panel.
fn outline_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("Library/Application Support/Knotes");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings-v2.json"), r#"{"panel": "outline"}"#).unwrap();
    home
}

/// The vault with `notes` and `files` probed with the outline shown.
fn probe_outline(notes: &[(&str, &str)], files: &[(&str, &[u8])], env: &[(&str, &str)]) -> String {
    let vault = tempfile::tempdir().unwrap();
    for (path, bytes) in notes.iter().map(|(p, t)| (*p, t.as_bytes())).chain(files.iter().copied()) {
        std::fs::write(vault.path().join(path), bytes).unwrap();
    }
    let home = outline_home();
    probe_env(&[vault.path()], Some(home.path()), env)
}

/// The outline's rows as shown, a dot per level under the top (`outline:` line).
fn outline_rows(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("outline: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

const HEADINGS: &str = "# A\ntext\n## B\n### C\n## D\nmore\n# E\nend";

#[test]
fn the_outline_lists_the_note_s_headings_nested() {
    let out = probe_outline(&[("a.md", HEADINGS)], &[], &[("KNOTES_PROBE_OPEN", "a.md")]);
    assert_eq!(outline_rows(&out), "A | ·B | ··C | ·D | E");
    let out = probe_outline(&[("a.md", "plain text")], &[], &[("KNOTES_PROBE_OPEN", "a.md")]);
    assert_eq!(outline_rows(&out), "");
    assert!(out.contains("outline-empty: Немає заголовків\n"), "{out}");
}

#[test]
fn the_outline_follows_the_note_as_it_is_edited() {
    let out = probe_outline(&[("a.md", HEADINGS)], &[], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "\n## F")]);
    assert_eq!(outline_rows(&out), "A | ·B | ··C | ·D | E | ·F");
}

#[test]
fn the_outline_of_a_pdf_is_its_bookmarks() {
    let outlined: &[u8] = include_bytes!("fixtures/outlined.pdf");
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    let out = probe_outline(&[("a.md", "See [[doc.pdf]] here\n\nend")], &[("doc.pdf", outlined)], &open);
    assert_eq!(outline_rows(&out), "Вступ | Part | ·Detail");
    let out = probe_outline(&[("a.md", "See [[doc.pdf]] here\n\nend")], &[("doc.pdf", PDF)], &open);
    assert!(out.contains("outline-empty: У цьому PDF немає змісту\n"), "{out}");
}

/// Three sections, each longer than the window, so each heading can reach the top.
fn long_sections() -> String {
    let filler = |name: &str| (0..80).map(|i| format!("{name} line {i}\n")).collect::<String>();
    format!("# A\n{}## B\n{}# C\n{}", filler("a"), filler("b"), filler("c"))
}

/// The first line at the top of the note's visible part (`note-top:` line).
fn note_top(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("note-top: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn a_click_on_a_heading_in_the_outline_scrolls_the_note_to_it() {
    let note = long_sections();
    let out = probe_outline(&[("a.md", &note)], &[], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_OUTLINE_CLICK", "B")]);
    assert_eq!(note_top(&out), "## B");
    let out = probe_outline(&[("a.md", &note)], &[], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_OUTLINE_CLICK", "C")]);
    assert_eq!(note_top(&out), "# C");
}

#[test]
fn a_click_on_a_bookmark_in_the_outline_shows_its_page_of_the_pdf() {
    let outlined: &[u8] = include_bytes!("fixtures/outlined.pdf");
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_OUTLINE_CLICK", "Detail")];
    let out = probe_outline(&[("a.md", "See [[doc.pdf]] here\n\nend")], &[("doc.pdf", outlined)], &env);
    assert!(out.contains("pdf-view: doc.pdf 3/3"), "{out}");
}

/// The heading highlighted in the outline (`outline-current:` line).
fn outline_current(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("outline-current: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn the_outline_highlights_the_heading_of_the_part_in_view() {
    let note = long_sections();
    let open = ("KNOTES_PROBE_OPEN", "a.md");
    assert_eq!(outline_current(&probe_outline(&[("a.md", &note)], &[], &[open])), "A");
    assert_eq!(outline_current(&probe_outline(&[("a.md", &note)], &[], &[open, ("KNOTES_PROBE_NOTE_SCROLL", "end")])), "C");
    assert_eq!(outline_current(&probe_outline(&[("a.md", &note)], &[], &[open, ("KNOTES_PROBE_OUTLINE_CLICK", "B")])), "B");
}

#[test]
fn after_a_click_in_the_outline_scrolling_on_highlights_the_heading_in_view_again() {
    let note = long_sections();
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_OUTLINE_CLICK", "A"), ("KNOTES_PROBE_NOTE_SCROLL", "end")];
    assert_eq!(outline_current(&probe_outline(&[("a.md", &note)], &[], &env)), "C");
}

#[test]
fn the_outline_of_a_pdf_highlights_the_bookmark_of_the_page_in_view() {
    let outlined: &[u8] = include_bytes!("fixtures/outlined.pdf");
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    let notes = [("a.md", "See [[doc.pdf]] here\n\nend")];
    assert_eq!(outline_current(&probe_outline(&notes, &[("doc.pdf", outlined)], &env)), "Вступ");
    // The last page cannot reach the top; the bookmark clicked stays highlighted.
    let out = probe_outline(&notes, &[("doc.pdf", outlined)], &[env[0], env[1], env[2], ("KNOTES_PROBE_OUTLINE_CLICK", "Detail")]);
    assert_eq!(outline_current(&out), "Detail");
}

#[test]
fn folds_in_the_outline_stay_as_the_note_is_edited() {
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_OUTLINE_FOLD", "B"), ("KNOTES_PROBE_TYPE", "\n## F")];
    assert_eq!(outline_rows(&probe_outline(&[("a.md", HEADINGS)], &[], &env)), "A | ·B | ·D | E | ·F");
}

#[test]
fn the_heading_in_view_folded_away_highlights_the_one_it_is_folded_under() {
    let note = long_sections();
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_OUTLINE_CLICK", "B"), ("KNOTES_PROBE_OUTLINE_FOLD", "A")];
    let out = probe_outline(&[("a.md", &note)], &[], &env);
    assert_eq!((outline_rows(&out), outline_current(&out)), ("A | C".to_owned(), "A".to_owned()));
}

#[test]
fn a_pdf_preview_widened_shows_the_old_picture_stretched_until_the_new_one_is_drawn() {
    // Widened after its page was drawn, by dragging its edge (the embed's line starts at 7).
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_RESIZE_LATER", "7:300")];
    let (_vault, out) = probe_images(&[("a.md", "Before\n![[doc.pdf|120]]\nAfter")], &[("doc.pdf", PDF)], &env);
    assert!(out.contains("images-shown: doc.pdf#page=1 stand-in\n"), "{out}");
}

/// `page:shows:width` for each page the viewer keeps, from a `name:` line.
fn pdf_drawn(out: &str, name: &str) -> Vec<(usize, String, f64)> {
    let line = out.lines().find_map(|l| l.strip_prefix(&format!("{name}: "))).unwrap_or_else(|| panic!("{out}"));
    line.split(' ')
        .map(|p| {
            let parts: Vec<&str> = p.split(':').collect();
            (parts[0].parse().unwrap(), parts[1].to_owned(), parts[2].parse().unwrap())
        })
        .collect()
}

#[test]
fn a_pdf_viewer_made_narrower_keeps_its_pages_until_the_width_settles_then_draws_them_again() {
    // The width changed 300 ms in, once the pages are drawn; the report 1.2 s in.
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_PDF_WIDTH", "300"), ("KNOTES_PROBE_DELAY_MS", "1200")];
    let (_vault, out) = probe_images(&[("a.md", "See [[doc.pdf]] here\n\nend")], &[("doc.pdf", PDF)], &env);
    let now = pdf_drawn(&out, "pdf-drawn-now");
    assert!(now.len() == 3 && now.iter().all(|(_, shows, width)| shows == "ready" && *width > 300.0), "as drawn before, stretched: {now:?}");
    let later = pdf_drawn(&out, "pdf-drawn");
    assert!(later.len() == 3 && later.iter().all(|(_, shows, width)| shows == "ready" && *width < 300.0), "drawn again for the width: {later:?}");
}

#[test]
fn a_pdf_viewer_made_narrower_keeps_the_page_at_the_top() {
    let long = pdf_of(30);
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_PDF_WIDTH", "300")];
    let (_vault, out) = probe_images(&[("a.md", "See [[long.pdf#page=15]] here\n\nend")], &[("long.pdf", &long)], &env);
    assert!(out.contains("pdf-top-now: 15\n"), "{out}");
}

#[test]
fn the_toolbar_names_a_pdf_open_by_its_file_name_without_its_folder() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "See [[paper.pdf]] here\n\nend").unwrap();
    std::fs::create_dir_all(vault.path().join(".knotes/attachments")).unwrap();
    std::fs::write(vault.path().join(".knotes/attachments/paper.pdf"), PDF).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")]);
    assert!(out.contains("pdf-view: .knotes/attachments/paper.pdf 1/3"), "{out}");
    assert!(out.contains("note-title: paper\n"), "without its folder or .pdf: {out}");
}

#[test]
fn a_double_click_on_the_name_in_the_toolbar_starts_renaming_and_a_single_click_does_not() {
    let editing = |clicks: &str| {
        let (_vault, out) = probe_notes(&RENAME_VAULT, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_TITLE_CLICKS", clicks)]);
        out.lines().find_map(|l| l.strip_prefix("note-title-editing: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
    };
    assert_eq!(editing("1"), "no");
    assert_eq!(editing("2"), "Old (all selected)");
    let (_vault, out) = probe_notes(&RENAME_VAULT, &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_TITLE_CLICKS", "1")]);
    assert!(out.contains("note-title-moves-window: no\n"), "the window must not take its clicks (a double click zoomed it): {out}");
}

#[test]
fn a_pdf_is_renamed_from_the_toolbar_and_links_to_it_follow() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "See [[paper.pdf]] here\n\nend").unwrap();
    std::fs::write(vault.path().join("paper.pdf"), PDF).unwrap();
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    let pdf = |more: (&str, &str)| probe_env(&[vault.path()], None, &[open[0], open[1], open[2], more]);
    let out = pdf(("KNOTES_PROBE_TITLE_CLICKS", "2"));
    assert!(out.contains("note-title-editing: paper (all selected)\n"), "{out}");
    let out = pdf(("KNOTES_PROBE_ACTION", "toolbar-rename=Report"));
    assert!(vault.path().join("Report.pdf").exists() && !vault.path().join("paper.pdf").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "See [[Report.pdf]] here\n\nend");
    assert!(out.contains("pdf-view: Report.pdf 1/3") && out.contains("note-title: Report\n"), "{out}");
}

/// A HOME of its own, kept between launches.
fn tabs_home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// `vault` probed with a HOME kept between launches (the tabs are along the toolbar).
fn probe_tabs(vault: &tempfile::TempDir, home: &tempfile::TempDir, env: &[(&str, &str)]) -> String {
    probe_env(&[vault.path()], Some(home.path()), env)
}

/// A vault with two notes and a PDF the first links to.
fn tabs_vault() -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "See [[doc.pdf]] here\n\nend").unwrap();
    std::fs::write(vault.path().join("b.md"), "b").unwrap();
    std::fs::write(vault.path().join("doc.pdf"), PDF).unwrap();
    vault
}

/// The tabs along the toolbar and the one shown (`tabs:` and `tab-shown:` lines).
fn sidebar_tabs(out: &str) -> (String, String) {
    let line = |name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("{out}")).trim_end().to_owned();
    (line("tabs:").trim_start().to_owned(), line("tab-shown: "))
}

#[test]
fn the_tab_closed_last_is_opened_again_by_its_key() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")];
    // b's tab closed by its cross: a is left. ⇧⌘T brings b back, beside a, and shows it.
    let out = probe_tabs(&vault, &home, &[open[0], open[1], ("KNOTES_PROBE_TAB_CLOSE", "b.md")]);
    assert_eq!(sidebar_tabs(&out), ("a".to_owned(), "a".to_owned()), "{out}");
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[open[0], open[1], ("KNOTES_PROBE_TAB_CLOSE", "b.md"), ("KNOTES_PROBE_MENU_KEY", "shift-t")]);
    assert!(out.contains("menu-key: Відкрити закриту вкладку\n"), "{out}");
    assert_eq!(sidebar_tabs(&out), ("a | b".to_owned(), "b".to_owned()), "{out}");
    assert!(out.contains("note: b.md"), "its document shows: {out}");
    // Nothing closed: the key does nothing.
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[open[0], open[1], ("KNOTES_PROBE_MENU_KEY", "shift-t")]);
    assert_eq!(sidebar_tabs(&out), ("a | b".to_owned(), "b".to_owned()), "{out}");
}

#[test]
fn a_secondary_click_on_a_tab_shows_the_tab_s_menu_not_the_toolbar_s() {
    // The window keeps a secondary click in its toolbar for the toolbar's own menu: the strip,
    // though the click lands on it, never heard of it, and the owner saw the toolbar's menu.
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md"), ("KNOTES_PROBE_TAB_RIGHT_CLICK", "a.md")]);
    assert!(out.contains("tab-right-click-hits: KnotesTabStrip\n"), "{out}");
    assert!(out.contains("tab-menu-shown: Закрити вкладку | Закрити інші вкладки\n"), "{out}");
}

#[test]
fn more_tabs_than_fit_keep_their_width_and_the_one_shown_is_in_view() {
    // Each PDF opens in a tab of its own: forty of them, far more than the strip holds.
    let vault = tempfile::tempdir().unwrap();
    let names: Vec<String> = (0..40).map(|i| format!("doc{i:02}.pdf")).collect();
    for name in &names {
        std::fs::write(vault.path().join(name), PDF).unwrap();
    }
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    let home = tabs_home();
    let places = |out: &str| out.lines().find_map(|l| l.strip_prefix("tab-places: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    // "40 tabs 150 wide, shown at 450..600 of 600": no tab squeezed to nothing, the last one
    // (shown) wholly inside the strip, at its right.
    let numbers = |line: &str| line.split(|c: char| !c.is_ascii_digit()).filter(|n| !n.is_empty()).map(|n| n.parse::<f64>().unwrap()).collect::<Vec<_>>();
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", &names.join(","))]);
    let line = places(&out);
    let n = numbers(&line);
    assert_eq!((n[0], n[1]), (40.0, 150.0), "{line}");
    assert!(n[2] >= 0.0 && n[3] <= n[4] && n[3] > n[4] - 2.0, "the one shown at the strip's right end: {line}");
    // The next launch shows the same tab; the first one clicked comes into view at the left.
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_TAB_CLICK", "doc00.pdf")]);
    let n = numbers(&places(&out));
    assert_eq!((n[0], n[2], n[3]), (40.0, 0.0, 150.0), "{}", places(&out));
    // A few tabs fill the strip as before, none scrolled.
    let (vault, home) = (tabs_vault(), tabs_home());
    let n = numbers(&places(&probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md,doc.pdf")])));
    assert!(n[0] == 2.0 && n[1] > 200.0 && n[2] >= 0.0 && n[3] <= n[4], "{n:?}");
}

#[test]
fn a_note_opens_in_the_tab_shown_and_a_pdf_in_a_new_tab_beside_it() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md,b.md")]);
    assert!(out.contains("sidebar-segments: Теки, Теги, Картки\n"), "the tabs are no list of the sidebar's: {out}");
    // The strip fills the toolbar between the sidebar's toggle and the panel's.
    let strip = out.lines().find_map(|l| l.strip_prefix("tab-strip: width=")).unwrap_or_else(|| panic!("{out}"));
    let (width, of) = strip.split_once(" of ").unwrap();
    let of = of.split(' ').next().unwrap();
    assert!(width.parse::<f64>().unwrap() > 0.7 * of.parse::<f64>().unwrap(), "{strip}");
    assert_eq!(sidebar_tabs(&out), ("b".to_owned(), "b".to_owned()), "b.md in a.md's tab");
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")]);
    assert_eq!(sidebar_tabs(&out), ("a | doc".to_owned(), "doc".to_owned()), "the PDF shown in a tab of its own: {out}");
}

#[test]
fn a_note_opens_in_a_new_tab_from_the_list_s_menu_and_a_document_with_a_tab_brings_it_back() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")]);
    assert_eq!(sidebar_tabs(&out), ("a | b".to_owned(), "b".to_owned()));
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md"), ("KNOTES_PROBE_TAB_CLICK", "a.md")]);
    assert_eq!(sidebar_tabs(&out), ("a | b".to_owned(), "a".to_owned()));
}

#[test]
fn the_buttons_after_the_tabs_search_the_vault_and_make_a_new_note() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let before = std::fs::read_dir(vault.path()).unwrap().count();
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TOOLBAR_BUTTON", "new-note")]);
    let made = std::fs::read_dir(vault.path()).unwrap().count() > before || vault.path().join("Вхідні").is_dir();
    assert!(made && !out.contains("note: a.md"), "a new note is made and shown, as the row's button does: {out}");
    // The magnifying glass opens the vault's search in a window of its own.
    let (vault, home) = (tabs_vault(), tabs_home());
    assert!(!probe_tabs(&vault, &home, &[]).contains("vault-search-window: open"));
    let searching = probe_tabs(&vault, &home, &[("KNOTES_PROBE_TOOLBAR_BUTTON", "search")]);
    assert!(searching.contains("vault-search-window: open\n") && !searching.contains("search-row: shown"), "not the list's search: {searching}");
    // The menu's item does the same, by its keys (⌥⌘F; ⇧⌘F is the list's search).
    let by_key = probe_tabs(&vault, &home, &[("KNOTES_PROBE_MENU_KEY", "alt-f")]);
    assert!(by_key.contains("menu-key: Пошук у базі…\n") && by_key.contains("vault-search-window: open\n"), "{by_key}");
}

#[test]
fn the_vault_s_search_lists_documents_by_name_then_by_text_and_shows_the_one_chosen() {
    let notes = [
        ("Projects/Radar.md", "# Radar\n\nThe radar project starts in May.\n\n## Budget\n\nThe budget of the radar grows next year.\n"),
        ("Сад.md", "Нотатка про сад і город.\n\nRadar is mentioned here once.\n"),
        ("Budget.md", "Budget notes only.\n"),
    ];
    let found = |out: &str| out.lines().find_map(|l| l.strip_prefix("vault-search: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    // By its name first, then by its text; the first is chosen and shown below, its words marked.
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_VAULT_SEARCH", "radar")]);
    assert_eq!(found(&out), "[Projects/Radar.md] | Сад.md ~ 3 marks, first «Radar», 95 letters shown", "{out}");
    assert!(!out.contains("note: Projects/Radar.md"), "nothing opens until asked: {out}");
    // Down chooses the next: its text below.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "radar"), ("KNOTES_PROBE_VAULT_SEARCH_DOWN", "1")]);
    assert_eq!(found(&out), "Projects/Radar.md | [Сад.md] ~ 1 marks, first «Radar», 56 letters shown", "{out}");
    // Every word, anywhere in the text; Return opens the document in the vault's window and
    // closes the search.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "budget grows"), ("KNOTES_PROBE_VAULT_SEARCH_OPEN", "1")]);
    assert!(found(&out).starts_with("[Projects/Radar.md] ~ 3 marks, first «Budget»"), "{out}");
    assert!(out.contains("note: Projects/Radar.md") && !out.contains("vault-search-window: open"), "{out}");
    // What is searched: the names alone, or the text alone.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "radar"), ("KNOTES_PROBE_VAULT_SEARCH_KIND", "names")]);
    assert!(found(&out).starts_with("[Projects/Radar.md] ~ "), "by its name only: {out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "budget"), ("KNOTES_PROBE_VAULT_SEARCH_KIND", "text")]);
    assert!(found(&out).contains("Projects/Radar.md") && found(&out).contains("Budget.md"), "{out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "budget"), ("KNOTES_PROBE_VAULT_SEARCH_KIND", "names")]);
    assert!(found(&out).starts_with("[Budget.md] ~ "), "{out}");
    // «Aa» off: the letters' case counts, in a name and in the text.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "radar"), ("KNOTES_PROBE_VAULT_SEARCH_CASE", "1")]);
    assert!(found(&out).starts_with("[Projects/Radar.md] ~ "), "«radar» small is in Radar's text, not in Сад's («Radar»): {out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "Radar"), ("KNOTES_PROBE_VAULT_SEARCH_CASE", "1")]);
    assert!(found(&out).contains("Projects/Radar.md") && found(&out).contains("Сад.md"), "{out}");
    // Nothing typed, nothing listed; words no document holds, none.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "")]);
    assert_eq!(found(&out), " ~ 0 marks, first «», 0 letters shown", "{out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "zzqq")]);
    assert!(found(&out).starts_with(" ~ 0 marks"), "{out}");
}

#[test]
fn the_vault_s_search_window_narrows_by_its_query_and_marks_the_words_alone() {
    // What narrows and how is tested without a window (knotes-app's `vault_search`): here, that
    // the window takes the query apart, knows the vault's spaces and marks the words alone.
    let notes = [
        (".knotes.json", r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}, {"id": "home", "name": "Дім", "color": "green"}]}"#),
        ("Projects/.knotes.json", r#"{"space": "work"}"#),
        ("Home/.knotes.json", r#"{"space": "home"}"#),
        ("Projects/Radar.md", "# Radar\n\nThe radar budget grows next year. #work\n"),
        ("Projects/Draft.md", "Radar draft: the year next to budget. #work/old\n"),
        ("Home/Сад.md", "Radar is mentioned here once. #garden\n"),
    ];
    let found = |out: &str| out.lines().find_map(|l| l.strip_prefix("vault-search: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_VAULT_SEARCH", r#"radar "next year" space:Робота OR space:Дім -draft"#)]);
    assert!(found(&out).starts_with("[Projects/Radar.md • Робота] ~ 4 marks, first «Radar»"), "the phrase's words marked, not its quotes nor the rest: {out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "space:Дім")]);
    assert!(found(&out).starts_with("[Home/Сад.md • Дім] ~ 0 marks"), "a place alone lists its documents: {out}");
}

#[test]
fn the_search_marks_the_words_in_its_rows_and_goes_through_a_document_s_finds() {
    let notes = [
        ("Projects/Radar.md", "# Radar\n\nThe radar project starts in May.\n\n## Budget\n\nThe budget of the radar grows next year.\n"),
        ("Сад.md", "Нотатка про сад і город.\n\nRadar is mentioned here once.\n"),
    ];
    let line = |out: &str, name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("{name}: {out}")).to_owned();
    // The words in a row's name and in its text found, whatever their case.
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_VAULT_SEARCH", "radar")]);
    let rows = line(&out, "vault-search-rows: ");
    assert!(rows.starts_with("[Radar] ~ [Radar] The [radar] project") && rows.contains(" | Сад ~ ") && rows.contains("[Radar] is mentioned"), "{rows}");
    // ⌘G goes on through the finds of the document shown, round its end; ⇧⌘G back.
    let find = |steps: &str| line(&probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "radar"), ("KNOTES_PROBE_VAULT_SEARCH_FIND", steps)]), "vault-search-find: ");
    assert_eq!(find("0"), "1 з 3 Radar", "the first shows as the document does");
    assert_eq!(find("1"), "2 з 3 radar");
    assert_eq!(find("3"), "1 з 3 Radar", "round the end");
    assert_eq!(find("-1"), "3 з 3 radar", "and back round the start");
    // A document found by its name alone has no find to go through.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "сад"), ("KNOTES_PROBE_VAULT_SEARCH_KIND", "names"), ("KNOTES_PROBE_VAULT_SEARCH_FIND", "1")]);
    assert_eq!(line(&out, "vault-search-find: "), "1 з 1 сад", "its word is in its text once: {out}");
}

#[test]
fn a_link_clicked_in_the_document_the_search_shows_opens_what_it_leads_to() {
    let notes = [("a.md", "See [[b]] here and [[nowhere]].\n"), ("b.md", "the target\n")];
    // A click inside `[[b]]`: b opens in the vault's window, the search closes.
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_VAULT_SEARCH", "here"), ("KNOTES_PROBE_VAULT_SEARCH_CLICK", "6")]);
    assert!(out.contains("vault-search: [a.md]") && out.contains("note: b.md") && !out.contains("vault-search-window: open"), "{out}");
    // A link to no note makes none from a search.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "here"), ("KNOTES_PROBE_VAULT_SEARCH_CLICK", "22")]);
    assert!(!out.contains("note: ") || !out.contains("note: nowhere.md"), "{out}");
    assert!(!vault.path().join("nowhere.md").exists(), "{out}");
    // A click on plain text follows nothing: the search stays.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_VAULT_SEARCH", "here"), ("KNOTES_PROBE_VAULT_SEARCH_CLICK", "1")]);
    assert!(out.contains("vault-search-window: open"), "{out}");
}

#[test]
fn the_keys_go_round_the_tabs_and_to_a_tab_by_its_number() {
    let (vault, home) = (tabs_vault(), tabs_home());
    std::fs::write(vault.path().join("c.md"), "c").unwrap();
    // Three tabs, the last shown.
    probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")]);
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_NEW_TAB", "c.md")]);
    assert_eq!(sidebar_tabs(&out), ("a | b | c".to_owned(), "c".to_owned()));
    let shown = |key: &str| sidebar_tabs(&probe_tabs(&vault, &home, &[("KNOTES_PROBE_MENU_KEY", key)])).1;
    // ⌃Tab from the last goes round to the first; ⌃⇧Tab back to the last; ⇧⌘[ the one before.
    assert_eq!(shown("ctrl-tab"), "a");
    assert_eq!(shown("ctrl-shift-tab"), "c");
    assert_eq!(shown("shift-["), "b");
    assert_eq!(shown("shift-]"), "c");
    // ⌘1 the first, ⌘2 the second, ⌘9 the last however many; a number with no tab, nothing.
    assert_eq!(shown("1"), "a");
    assert_eq!(shown("2"), "b");
    assert_eq!(shown("9"), "c");
    assert_eq!(shown("5"), "c");
}

#[test]
fn a_tab_dragged_takes_its_new_place_and_keeps_it() {
    let (vault, home) = (tabs_vault(), tabs_home());
    std::fs::write(vault.path().join("c.md"), "c").unwrap();
    probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")]);
    probe_tabs(&vault, &home, &[("KNOTES_PROBE_NEW_TAB", "c.md")]);
    // a dragged to the end: the others close up; c, shown, stays shown.
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_TAB_DRAG", "a.md:2")]);
    assert_eq!(sidebar_tabs(&out), ("b | c | a".to_owned(), "c".to_owned()), "{out}");
    // The next launch has them so, and the keys count them so.
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_MENU_KEY", "1")]);
    assert_eq!(sidebar_tabs(&out), ("b | c | a".to_owned(), "b".to_owned()), "{out}");
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_TAB_DRAG", "c.md:0")]);
    assert_eq!(sidebar_tabs(&out).0, "c | b | a", "{out}");
}

#[test]
fn a_click_on_a_tab_shows_its_document() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md"), ("KNOTES_PROBE_TAB_CLICK", "a.md")]);
    assert!(out.contains("note: a.md"), "{out}");
    assert_eq!(sidebar_tabs(&out).1, "a");
}

#[test]
fn closing_another_tab_keeps_the_document_shown() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md"), ("KNOTES_PROBE_TAB_CLOSE", "a.md")]);
    assert_eq!(sidebar_tabs(&out), ("b".to_owned(), "b".to_owned()));
    assert!(out.contains("note-title: b\n"), "{out}");
}

#[test]
fn closing_the_tab_shown_shows_its_neighbour_and_closing_the_last_shows_nothing() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md"), ("KNOTES_PROBE_TAB_CLOSE", "b.md")]);
    assert_eq!(sidebar_tabs(&out), ("a".to_owned(), "a".to_owned()));
    assert!(out.contains("note-title: a\n") && out.contains("text: See [[doc.pdf]] here"), "{out}");
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TAB_CLOSE", "a.md")]);
    assert_eq!(sidebar_tabs(&out), ("".to_owned(), "none".to_owned()));
    assert!(out.contains("note-title: \n") && out.contains("text: \n"), "{out}");
}

#[test]
fn a_tab_brings_its_note_back_where_it_was() {
    let (vault, home) = (tabs_vault(), tabs_home());
    std::fs::write(vault.path().join("a.md"), long_sections()).unwrap();
    // Scrolled to the end 300 ms in; then b's tab and a's again.
    let env = [
        ("KNOTES_PROBE_OPEN", "a.md"),
        ("KNOTES_PROBE_CURSOR", "40"),
        ("KNOTES_PROBE_NEW_TAB", "b.md"),
        ("KNOTES_PROBE_TAB_CLICK", "a.md"),
        ("KNOTES_PROBE_NOTE_SCROLL", "end-later"),
        ("KNOTES_PROBE_TAB_TRIP", "b.md"),
    ];
    let out = probe_tabs(&vault, &home, &env);
    let before = out.lines().find_map(|l| l.strip_prefix("note-top-before-trip: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    assert_ne!(before, "# A", "scrolled: {out}");
    assert_eq!(note_top(&out), before, "{out}");
    assert!(out.contains("cursor: 40\n"), "{out}");
}

#[test]
fn a_tab_brings_its_pdf_back_at_the_page_it_was_at() {
    let (vault, home) = (tabs_vault(), tabs_home());
    std::fs::write(vault.path().join("a.md"), "See [[long.pdf#page=15]] here\n\nend").unwrap();
    std::fs::write(vault.path().join("long.pdf"), pdf_of(30)).unwrap();
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_TAB_TRIP", "a.md")];
    let out = probe_tabs(&vault, &home, &env);
    assert!(out.contains("pdf-view: long.pdf 15/30") && out.contains("pdf-top: 15\n"), "{out}");
}

#[test]
fn tabs_and_where_their_documents_were_come_back_after_a_restart() {
    let (vault, home) = (tabs_vault(), tabs_home());
    std::fs::write(vault.path().join("a.md"), long_sections()).unwrap();
    // The cursor put in a, b opened beside it, a's tab clicked and a scrolled.
    let env = [
        ("KNOTES_PROBE_OPEN", "a.md"),
        ("KNOTES_PROBE_CURSOR", "40"),
        ("KNOTES_PROBE_NEW_TAB", "b.md"),
        ("KNOTES_PROBE_TAB_CLICK", "a.md"),
        ("KNOTES_PROBE_NOTE_SCROLL", "end-later"),
    ];
    let first = probe_tabs(&vault, &home, &env);
    let out = probe_tabs(&vault, &home, &[]);
    assert_eq!(sidebar_tabs(&out), ("a | b".to_owned(), "a".to_owned()), "{out}");
    assert!(out.contains("note-title: a\n") && out.contains("cursor: 40\n"), "{out}");
    assert_eq!(note_top(&out), note_top(&first));
}

#[test]
fn closing_the_tab_shown_does_not_give_its_place_to_the_next() {
    let (vault, home) = (tabs_vault(), tabs_home());
    std::fs::write(vault.path().join("a.md"), long_sections()).unwrap();
    std::fs::write(vault.path().join("b.md"), format!("b start\n{}", "b line\n".repeat(300))).unwrap();
    // b's place is its top; a scrolled down, then a's tab closed: b shows from its top.
    let env = [("KNOTES_PROBE_OPEN", "b.md"), ("KNOTES_PROBE_NEW_TAB", "a.md"), ("KNOTES_PROBE_NOTE_SCROLL", "end-later"), ("KNOTES_PROBE_TAB_CLOSE", "a.md")];
    let out = probe_tabs(&vault, &home, &env);
    assert!(out.contains("note-title: b\n"), "{out}");
    assert_eq!(note_top(&out), "b start");
}

#[test]
fn a_deleted_document_loses_its_tab_and_a_renamed_one_keeps_it() {
    let (vault, home) = (tabs_vault(), tabs_home());
    probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")]);
    std::fs::remove_file(vault.path().join("a.md")).unwrap();
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_ACTION", "toolbar-rename=c")]);
    assert_eq!(sidebar_tabs(&out).0, "c", "{out}");
}

#[test]
fn a_link_s_context_menu_opens_its_document_in_a_new_tab() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "See [[b]] or https://example.com here\n\nend").unwrap();
    std::fs::write(vault.path().join("b.md"), "b").unwrap();
    let home = tabs_home();
    let menu = |at: &str| probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_LINK_MENU", at)]);
    let out = menu("6");
    assert!(out.contains("link-menu: Відкрити в новій вкладці\n"), "{out}");
    assert!(menu("20").contains("link-menu: none\n"), "a web link: none");
    let out = menu("6:open");
    assert_eq!(sidebar_tabs(&out), ("a | b".to_owned(), "b".to_owned()), "{out}");
}

#[test]
fn cmd_w_closes_the_tab_shown() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md"), ("KNOTES_PROBE_MENU_KEY", "w")]);
    assert!(out.contains("menu-key: Закрити вкладку\n"), "{out}");
    assert_eq!(sidebar_tabs(&out), ("a".to_owned(), "a".to_owned()), "{out}");
    assert!(out.contains("window-menu: Закрити вікно ⇧⌘W\n"), "the window closes with Shift: {out}");
}

#[test]
fn a_tab_s_menu_closes_it_or_the_others() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let menu = |spec: &str| {
        let mut env = vec![("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")];
        env.push(("KNOTES_PROBE_TAB_MENU", spec));
        probe_tabs(&vault, &home, &env)
    };
    let out = menu("a.md");
    assert!(out.contains("tab-menu: Закрити вкладку | Закрити інші вкладки\n"), "{out}");
    let out = menu("a.md:others");
    assert_eq!(sidebar_tabs(&out), ("a".to_owned(), "a".to_owned()), "b's tab closed, a shown: {out}");
    let out = menu("a.md:close");
    assert_eq!(sidebar_tabs(&out), ("b".to_owned(), "b".to_owned()), "{out}");
}

/// Whether each tab's cross shows, in order (`tab-crosses:` line).
fn tab_crosses(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("tab-crosses: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn a_tab_s_cross_shows_only_with_the_pointer_over_its_row() {
    let (vault, home) = (tabs_vault(), tabs_home());
    let two = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_NEW_TAB", "b.md")];
    assert_eq!(tab_crosses(&probe_tabs(&vault, &home, &two)), "hidden hidden");
    let out = probe_tabs(&vault, &home, &[two[0], two[1], ("KNOTES_PROBE_TAB_HOVER", "a.md")]);
    assert_eq!(tab_crosses(&out), "shown hidden");
    let out = probe_tabs(&vault, &home, &[two[0], two[1], ("KNOTES_PROBE_TAB_HOVER", "a.md:out")]);
    assert_eq!(tab_crosses(&out), "hidden hidden", "the pointer left");
}

#[test]
fn code_in_a_block_with_a_language_is_coloured_by_kind() {
    let note = "Intro\n\n```rust\nfn main() { let n = 42; } // done\n```\n\n```\nfn plain() {}\n```\n\nend";
    let (_vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let editor = out.lines().find_map(|l| l.strip_prefix("editor: ")).unwrap_or_else(|| panic!("{out}"));
    for token in ["[code-keyword]fn", "[code-function]main", "[code-keyword]let", "[code-number]42", "[code-comment]// done"] {
        assert!(editor.contains(token), "{token}: {editor}");
    }
    assert!(!editor.contains("[code-keyword]fn plain") && !editor.contains("[code-function]plain"), "no language, no colours: {editor}");
}

/// `decoration: code-number N x y w h` lines: each number and its x and width.
fn code_numbers(out: &str) -> Vec<(usize, f64, f64)> {
    out.lines()
        .filter_map(|l| l.strip_prefix("decoration: code-number "))
        .map(|l| {
            let p: Vec<&str> = l.split(' ').collect();
            (p[0].parse().unwrap(), p[1].parse().unwrap(), p[3].parse().unwrap())
        })
        .collect()
}

#[test]
fn code_lines_are_numbered_in_a_gutter_left_of_the_code() {
    let note = "text\n\n```rust\nlet a = 1;\nlet b = 2;\nlet c = 3;\n```\n\nend";
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let numbers = code_numbers(&out);
    assert_eq!(numbers.iter().map(|n| n.0).collect::<Vec<_>>(), [1, 2, 3], "the fences have none: {out}");
    let back = decorations(&out, "code");
    assert!(numbers.iter().all(|&(_, x, w)| x > back[0].0 && x + w < back[0].0 + 50.0), "in the block's gutter: {numbers:?} {back:?}");
}

#[test]
fn a_code_block_names_its_language_at_its_top_right() {
    let note = "text\n\n```rust\nlet a = 1;\n```\n\n```\nplain\n```\n\nend";
    let (_vault, out) = probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let labels: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("decoration: code-label ")).collect();
    assert_eq!(labels.len(), 1, "the block without a language has none: {out}");
    assert!(labels[0].starts_with("rust "), "{labels:?}");
    let (x, w) = {
        let p: Vec<f64> = labels[0].split(' ').skip(1).map(|v| v.parse().unwrap()).collect();
        (p[0], p[2])
    };
    let back = decorations(&out, "code");
    assert!((back[0].0 + back[0].2) - (x + w) < 20.0, "at the right: {labels:?} {back:?}");
}

#[test]
fn the_pointer_over_a_code_block_shows_a_copy_button_that_copies_its_code() {
    let note = "text\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n\nend";
    let inside = note.find("let b").unwrap().to_string();
    let probe = |more: (&str, &str)| probe_notes(&[("c.md", note)], &[("KNOTES_PROBE_OPEN", "c.md"), ("KNOTES_PROBE_CURSOR", "end"), more]).1;
    assert!(decorations(&probe(("KNOTES_PROBE_CURSOR", "end")), "code-copy").is_empty(), "no pointer, no button");
    assert_eq!(decorations(&probe(("KNOTES_PROBE_CODE_HOVER", &inside)), "code-copy").len(), 1);
    // Copied to a pasteboard of the probe's own, never the user's.
    let out = probe(("KNOTES_PROBE_CODE_COPY", &inside));
    assert!(out.contains("code-copied: let a = 1;⏎let b = 2;\n"), "{out}");
}

/// What the note column shows (`note-column:` line).
fn note_column(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("note-column: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn with_no_document_open_the_note_column_says_so_and_takes_no_typing() {
    let (vault, home) = (tabs_vault(), tabs_home());
    assert_eq!(note_column(&probe_tabs(&vault, &home, &[])), "empty: Немає відкритого документа");
    assert_eq!(note_column(&probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md")])), "note (editable)");
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TAB_CLOSE", "a.md")]);
    assert_eq!(note_column(&out), "empty: Немає відкритого документа", "the last tab closed: {out}");
    let out = probe_tabs(&vault, &home, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")]);
    assert_eq!(note_column(&out), "pdf");
}

/// How many places the PDF viewer highlights as found (`pdf-found:` line).
fn pdf_found(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("pdf-found: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn cmd_f_finds_words_in_a_pdf_and_steps_through_them() {
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8"), ("KNOTES_PROBE_FIND", "page")];
    let notes = [("a.md", "See [[doc.pdf]] here\n\nend")];
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &open);
    assert!(out.contains("find-bar: shown") && out.contains("find: 1 з 3\n"), "{out}");
    assert_eq!(pdf_found(&out), "3, the current on page 1");
    assert!(out.contains("pdf-find-bar: over the pages\n"), "the bar is not under the viewer: {out}");
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &[open[0], open[1], open[2], open[3], ("KNOTES_PROBE_PDF_FIND_STEPS", "2")]);
    assert!(out.contains("find: 3 з 3\n") && out.contains("pdf-view: doc.pdf 3/3"), "{out}");
    let (_vault, out) = probe_images(&notes, &[("doc.pdf", PDF)], &[open[0], open[1], open[2], ("KNOTES_PROBE_FIND", "nothing")]);
    assert!(out.contains("find: Немає\n"), "{out}");
}

#[test]
fn a_pdf_opened_from_a_search_shows_the_page_its_words_are_on() {
    let long = pdf_of(30);
    let env = [("KNOTES_PROBE_SEARCH", "content: 25"), ("KNOTES_PROBE_OPEN", "long.pdf")];
    let (_vault, out) = probe_images(&[("a.md", "a note")], &[("long.pdf", &long)], &env);
    assert!(out.contains("pdf-view: long.pdf 25/30"), "{out}");
    assert_eq!(pdf_found(&out), "1, the current on page 25", "the word highlighted");
    assert!(out.contains("find-bar: shown") && out.contains("find-field: 25\n") && out.contains("find: 1 з 1\n"), "Cmd+F's bar with the word: {out}");
    assert!(!out.contains("focus: find"), "the focus stays where it was (arrows through the results): {out}");
}

/// A HOME whose settings show the connections in the right panel.
fn links_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("Library/Application Support/Knotes");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings-v2.json"), r#"{"panel": "links"}"#).unwrap();
    home
}

/// A vault where `Plan.md` is linked from two notes, named in a third, and links out itself.
fn links_vault() -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    let write = |p: &str, t: &str| std::fs::write(vault.path().join(p), t).unwrap();
    write("Plan.md", "# Plan\n\nSee [[Other]] and [[Missing note]].");
    write("Daily.md", "Today: [[Plan]] is due.\nAnd [[Plan|the plan]] again.");
    write("Week.md", "Week starts. [[Plan]]");
    write("Idea.md", "A rough plan for later.");
    write("Other.md", "other");
    vault
}

/// The connections panel's rows (`connections:` line).
fn connections(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("connections: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn the_connections_list_links_here_unlinked_mentions_and_outgoing_links() {
    let (vault, home) = (links_vault(), links_home());
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Plan.md")]);
    assert!(out.contains("panel: links, shows links"), "{out}");
    assert_eq!(
        connections(&out),
        "[Посилання сюди · 3] | Daily (2) | ·Today: «[[Plan]]» is due. | ·And «[[Plan|the plan]]» again. | Week (1) | ·Week starts. «[[Plan]]» \
         | [Згадки без посилань · 1] | Idea (1) | ·A rough «plan» for later. ↗ \
         | [Вихідні · 2] | Other | Missing note (немає)"
    );
}

#[test]
fn a_click_on_a_line_in_the_connections_opens_its_note_at_the_link() {
    let (vault, home) = (links_vault(), links_home());
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Plan.md"), ("KNOTES_PROBE_CONNECTION_CLICK", "And «[[Plan|the plan]]» again.")]);
    assert!(out.contains("note: Daily.md"), "{out}");
    let at = "Today: [[Plan]] is due.\nAnd ".encode_utf16().count();
    assert!(out.contains(&format!("selection: {at} {}", "[[Plan|the plan]]".encode_utf16().count())), "the link selected: {out}");
}

#[test]
fn an_unlinked_mention_is_made_a_link_from_the_connections() {
    let (vault, home) = (links_vault(), links_home());
    // The button 500 ms in, once the connections are listed; the report waits for the index to
    // take in the note changed.
    let env = [("KNOTES_PROBE_OPEN", "Plan.md"), ("KNOTES_PROBE_CONNECTION_LINK", "Idea"), ("KNOTES_PROBE_DELAY_MS", "2000")];
    let out = probe_env(&[vault.path()], Some(home.path()), &env);
    assert_eq!(std::fs::read_to_string(vault.path().join("Idea.md")).unwrap(), "A rough [[Plan|plan]] for later.");
    assert!(connections(&out).contains("[Згадки без посилань · 0]") && connections(&out).contains("Idea (1) | ·A rough «[[Plan|plan]]» for later."), "{out}");
}

#[test]
fn many_notes_linking_here_are_listed_folded() {
    let vault = links_vault();
    for i in 0..25 {
        std::fs::write(vault.path().join(format!("Ref {i:02}.md")), "Points at [[Plan]].").unwrap();
    }
    let home = links_home();
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Plan.md")]);
    let rows = connections(&out);
    assert!(rows.starts_with("[Посилання сюди · 28] | Daily (2) | Ref 00 (1) | Ref 01 (1)"), "folded: notes without their lines: {rows}");
    assert!(!rows.contains("·Points at"), "{rows}");
}

#[test]
fn past_fifty_notes_linking_here_the_rest_show_on_asking() {
    let vault = links_vault();
    for i in 0..60 {
        std::fs::write(vault.path().join(format!("Ref {i:02}.md")), "Points at [[Plan]].").unwrap();
    }
    let home = links_home();
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Plan.md")]);
    let rows = connections(&out);
    assert!(rows.contains("Ref 48 (1) | Ще 12… | [Згадки без посилань"), "the first fifty, then the rest to ask for: {rows}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Plan.md"), ("KNOTES_PROBE_CONNECTION_CLICK", "Ще 12…")]);
    let rows = connections(&out);
    assert!(rows.contains("Ref 59 (1) | Week (1) | [Згадки без посилань") && !rows.contains("Ще "), "{rows}");
}

/// The 30-page PDF opened from a link, then `more` probe settings.
fn long_pdf(more: &[(&str, &str)]) -> String {
    let long = pdf_of(30);
    let mut env = vec![("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    env.extend_from_slice(more);
    probe_images(&[("a.md", "See [[long.pdf]] here\n\nend")], &[("long.pdf", &long)], &env).1
}

/// How far the PDF viewer is scrolled (`pdf-scroll:` line).
fn pdf_scroll(out: &str) -> f64 {
    out.lines().find_map(|l| l.strip_prefix("pdf-scroll: ")).unwrap_or_else(|| panic!("{out}")).parse().unwrap()
}

#[test]
fn a_pdf_opened_from_a_note_takes_the_keys_and_arrows_turn_its_pages() {
    let out = long_pdf(&[]);
    assert!(out.contains("focus: pdf"), "opened from the note: the keys go to the PDF: {out}");
    assert!(long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "right,right")]).contains("pdf-view: long.pdf 3/30"));
    assert!(long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "right,right,left")]).contains("pdf-view: long.pdf 2/30"));
    assert!(long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "cmd-down")]).contains("pdf-view: long.pdf 30/30"));
    assert!(long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "end,home")]).contains("pdf-view: long.pdf 1/30"));
}

#[test]
fn space_and_page_keys_scroll_a_pdf_by_a_screen_and_arrows_by_a_little() {
    let top = pdf_scroll(&long_pdf(&[]));
    let screen = pdf_scroll(&long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "space")])) - top;
    assert!(screen > 300.0, "about a screen: {screen}");
    assert!((pdf_scroll(&long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "pagedown")])) - top - screen).abs() < 1.0, "Page Down as space");
    assert!((pdf_scroll(&long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "space,shift-space")])) - top).abs() < 1.0, "Shift+space back");
    let little = pdf_scroll(&long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "down")])) - top;
    assert!(little > 10.0 && little < 100.0, "an arrow: a little: {little}");
}

#[test]
fn the_view_menu_zooms_a_pdf_in_out_and_back() {
    let zoom = |steps: &str| {
        let out = long_pdf(&[("KNOTES_PROBE_PDF_ZOOM", steps)]);
        out.lines().find_map(|l| l.strip_prefix("pdf-zoom: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
    };
    assert_eq!(zoom("in"), "1.25");
    assert_eq!(zoom("in,in"), "1.56");
    assert_eq!(zoom("out"), "0.80");
    assert_eq!(zoom("in,in,actual"), "1.00");
}

/// The PDF viewer's page label and whether it shows (`pdf-page-label:` line).
fn page_label(out: &str) -> String {
    out.lines().find_map(|l| l.strip_prefix("pdf-page-label: ")).unwrap_or_else(|| panic!("{out}")).to_owned()
}

#[test]
fn a_pdf_shows_its_page_and_page_count_as_it_moves_then_hides_them() {
    assert_eq!(page_label(&long_pdf(&[])), "сторінка 1 з 30 (shown)", "on opening");
    assert_eq!(page_label(&long_pdf(&[("KNOTES_PROBE_PDF_KEYS", "right,right")])), "сторінка 3 з 30 (shown)");
    assert!(page_label(&long_pdf(&[("KNOTES_PROBE_DELAY_MS", "2500")])).ends_with("(hidden)"), "a moment after the last move");
}

/// OpenAI's streamed reply with `output`, as the chat's probe transport plays it.
fn chat_reply(output: &[serde_json::Value]) -> serde_json::Value {
    let mut events = Vec::new();
    for item in output {
        if let Some(text) = item["content"][0]["text"].as_str() {
            events.push(serde_json::json!({"type": "response.output_text.delta", "delta": text}));
        }
        events.push(serde_json::json!({"type": "response.output_item.done", "item": item}));
    }
    events.push(serde_json::json!({"type": "response.completed", "response": {"output": output, "usage": {"input_tokens": 1000, "output_tokens": 5}}}));
    let body: String = events.iter().map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap())).collect();
    serde_json::json!({"status": 200, "body": body})
}

fn chat_message(text: &str) -> serde_json::Value {
    serde_json::json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]})
}

/// A file of OpenAI's answers for the chat, kept with the vault's temporary folder.
fn chat_script(dir: &std::path::Path, answers: &[serde_json::Value]) -> String {
    let path = dir.join("chat-script.json");
    std::fs::write(&path, serde_json::Value::Array(answers.to_vec()).to_string()).unwrap();
    path.to_string_lossy().into_owned()
}

fn radar_answers() -> Vec<serde_json::Value> {
    let call = serde_json::json!({"type": "function_call", "call_id": "c1", "name": "search", "arguments": "{\"query\":\"radar\"}"});
    vec![chat_reply(&[call]), chat_reply(&[chat_message("Radar starts in **May**, see [[Radar]].")])]
}

#[test]
fn an_answers_formulas_are_typeset_in_the_chat() {
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &[chat_reply(&[chat_message(r"For \(x\ne a\) it is $2a$, not \(\nosuch{1}\).")])]);
    let (_vault, out) = probe_notes(&[("Boris.md", "# Boris")], &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_SEND", "How?"), ("KNOTES_PROBE_DELAY_MS", "2000")]);
    assert!(out.contains("chat transcript: How?⏎⏎For \u{fffc} it is \u{fffc}, not \\nosuch{1}.\n"), "a picture for each formula, the source of one that does not typeset: {out}");
    assert!(out.contains("chat copied: How?⏎⏎For $x\\ne a$ it is $2a$, not \\nosuch{1}.\n"), "copied, a formula is its source: {out}");
}

#[test]
fn a_question_streams_its_answer_with_the_tools_it_used_and_note_links() {
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &radar_answers());
    let (_vault, out) = probe_notes(
        &[("Radar.md", "# Radar\n\nStarts in May."), ("Boris.md", "# Boris")],
        &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_SEND", "When does radar start?"), ("KNOTES_PROBE_DELAY_MS", "2500")],
    );
    assert!(out.contains("chat: signed in | model: Probe Model | effort: Звичайні роздуми | document: Boris  ✕ | Використовується план ChatGPT"), "the model's own effort: {out}");
    assert!(out.contains("chat transcript: When does radar start?⏎⏎Пошук: radar⏎⏎Radar starts in May, see Radar.\n"), "{out}");
    assert!(out.contains("chat links: knotes-note:Radar\n"), "{out}");
    assert!(out.contains("chat bubbles: 1 | When does radar start?\n"), "the question stands apart from the answer: {out}");
    let context = out.lines().find(|l| l.starts_with("chat context: ")).unwrap_or_default();
    assert!(context.starts_with("chat context: 10") && context.ends_with(" of 244800"), "OpenAI's count, and 90 % of the 272K window: {context}");
    assert!(out.contains("chat conversations: When does radar start? | selected 0"), "{out}");
    assert!(out.contains("chat field: \n"), "{out}");
}

#[test]
fn the_effort_chosen_is_shown_and_kept_for_the_next_launch() {
    let (home, script_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let script = chat_script(script_dir.path(), &radar_answers());
    let notes = [("Radar.md", "# Radar\n\nStarts in May.")];
    let chosen = [("KNOTES_PROBE_CHAT", script.as_str()), ("KNOTES_PROBE_CHAT_EFFORT", "high"), ("KNOTES_PROBE_DELAY_MS", "1500")];
    let (_vault, out) = probe_notes_at_home(&notes, &chosen, home.path());
    assert!(out.contains("| effort: Довгі роздуми |"), "{out}");
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_CHAT", script.as_str()), ("KNOTES_PROBE_DELAY_MS", "1500")], home.path());
    assert!(out.contains("| effort: Довгі роздуми |"), "kept: {out}");
}

#[test]
fn a_note_link_in_an_answer_opens_the_note() {
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &radar_answers());
    let (_vault, out) = probe_notes(
        &[("Radar.md", "# Radar\n\nStarts in May."), ("Boris.md", "# Boris")],
        &[
            ("KNOTES_PROBE_OPEN", "Boris.md"),
            ("KNOTES_PROBE_CHAT", &script),
            ("KNOTES_PROBE_CHAT_SEND", "When does radar start?"),
            ("KNOTES_PROBE_CHAT_CLICK_LINK", "1"),
            ("KNOTES_PROBE_DELAY_MS", "2500"),
        ],
    );
    assert!(out.contains("note: Radar.md | # Radar⏎⏎Starts in May."), "{out}");
}

#[test]
fn the_document_offered_to_the_chat_can_be_crossed_out() {
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &[]);
    let (_vault, out) = probe_notes(&[("Boris.md", "# Boris")], &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_TOGGLE_DOC", "1")]);
    assert!(out.contains("| document: + Boris |"), "{out}");
}

#[test]
fn a_usage_limit_is_said_with_a_link_to_manage_usage_and_the_question_kept() {
    let script_dir = tempfile::tempdir().unwrap();
    let limit = serde_json::json!({"status": 429, "body": serde_json::json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "limit"}}).to_string()});
    let script = chat_script(script_dir.path(), &[limit]);
    let (_vault, out) = probe_notes(
        &[("Boris.md", "# Boris")],
        &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_SEND", "When?"), ("KNOTES_PROBE_DELAY_MS", "2000")],
    );
    assert!(out.contains("chat transcript: Ліміт використання плану ChatGPT вичерпано. Керувати використанням\n"), "{out}");
    assert!(out.contains("chat links: knotes-web:https://chatgpt.com/settings/usage"), "{out}");
    assert!(out.contains("chat field: When?\n"), "{out}");
    assert!(out.contains("chat conversations: Нова розмова | selected 0"), "{out}");
}

#[test]
fn conversations_are_kept_per_vault_and_listed_after_a_relaunch() {
    let (home, vault, script_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(vault.path().join("Boris.md"), "# Boris").unwrap();
    let script = chat_script(script_dir.path(), &[chat_reply(&[chat_message("First answer.")]), chat_reply(&[chat_message("Second answer.")])]);
    let env = |extra: &[(&'static str, &'static str)]| {
        let mut env = vec![("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", script.as_str())];
        env.extend_from_slice(extra);
        probe_env(&[vault.path()], Some(home.path()), &env)
    };
    let out = env(&[("KNOTES_PROBE_CHAT_SEND", "First?|And then?"), ("KNOTES_PROBE_DELAY_MS", "2500")]);
    assert!(out.contains("chat transcript: First?⏎⏎First answer.⏎⏎And then?⏎⏎Second answer.\n"), "{out}");
    assert!(out.contains("chat bubbles: 2 | First? | And then?\n"), "{out}");

    let out = env(&[]);
    assert!(out.contains("chat conversations: First? | selected 0"), "{out}");
    assert!(out.contains("chat transcript: First?⏎⏎First answer.⏎⏎And then?⏎⏎Second answer.\n"), "{out}");

    let out = env(&[("KNOTES_PROBE_CHAT_NEW", "1")]);
    assert!(out.contains("chat conversations: Нова розмова, First? | selected 0"), "{out}");
    assert!(out.contains("chat transcript: \n"), "{out}");

    let out = env(&[("KNOTES_PROBE_CHAT_NEW", "1"), ("KNOTES_PROBE_CHAT_CHOOSE", "1")]);
    assert!(out.contains("chat conversations: First? | selected 0"), "{out}");
}

#[test]
fn every_point_of_the_question_field_takes_a_click() {
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &[]);
    let (_vault, out) = probe_notes(&[("Boris.md", "# Boris")], &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_FIELD_CLICKS", "1")]);
    let line = out.lines().find(|l| l.starts_with("chat field clicks:")).unwrap_or_else(|| panic!("{out}"));
    assert!(line.contains("text 55, button 1, missed [] | text height 66"), "{line}");
}

#[test]
fn where_a_long_conversation_was_summarized_for_the_model_is_marked() {
    let (home, vault, script_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(vault.path().join("Boris.md"), "# Boris").unwrap();
    let dir = knotes_session::index_dir(&home.path().join("Library/Application Support/Knotes"), vault.path()).join("conversations");
    std::fs::create_dir_all(&dir).unwrap();
    let say = |role: &str, kind: &str, text: &str| serde_json::json!({"type": "message", "role": role, "content": [{"type": kind, "text": text}]});
    let conversation = serde_json::json!({
        "id": "abc1", "title": "Q1", "model": "probe-model", "updated": 100,
        "items": [say("user", "input_text", "Q1"), say("assistant", "output_text", "A1"), say("user", "input_text", "Q2"), say("assistant", "output_text", "A2")],
        "summary": {"covers": 2, "text": "Asked Q1."},
    });
    std::fs::write(dir.join("abc1.json"), conversation.to_string()).unwrap();
    std::fs::write(dir.join("index.json"), serde_json::json!([{"id": "abc1", "title": "Q1", "updated": 100}]).to_string()).unwrap();
    let script = chat_script(script_dir.path(), &[]);
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script)]);
    assert!(out.contains("chat transcript: Q1⏎⏎A1⏎⏎— вище розмову стиснуто: ШІ пам'ятає її коротко —⏎⏎Q2⏎⏎A2\n"), "{out}");
}

#[test]
fn ask_the_ai_shows_the_chat_and_focuses_its_question_field() {
    let (home, vault, script_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(vault.path().join("Boris.md"), "# Boris").unwrap();
    let script = chat_script(script_dir.path(), &[]);
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_ACTION", "panel=outline")]);
    assert!(out.contains("panel: outline"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script)]);
    assert!(out.contains("chat field focused: no"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_ASK_AI", "1")]);
    assert!(out.contains("panel: ai, shows chat"), "{out}");
    assert!(out.contains("chat field focused: yes"), "{out}");
}

#[test]
fn the_view_menu_asks_the_ai_with_cmd_l() {
    let out = probe();
    assert!(out.contains("Запитати ШІ ⌘L"), "{out}");
}

#[test]
fn code_in_an_answer_sits_on_a_card_of_its_own() {
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &[chat_reply(&[chat_message("Run:\n\n```sh\nls\npwd\n```\n\nThen `cd` back.")])]);
    let (_vault, out) = probe_notes(&[("Boris.md", "# Boris")], &[("KNOTES_PROBE_OPEN", "Boris.md"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_SEND", "How?"), ("KNOTES_PROBE_DELAY_MS", "2000")]);
    assert!(out.contains("chat transcript: How?⏎⏎Run:⏎⏎ls⏎pwd⏎⏎Then cd back.\n"), "code keeps its line breaks: {out}");
    assert!(out.contains("chat code: [ls pwd]\n"), "one card, rounded at its first and last lines: {out}");
}

#[test]
fn test_questions_run_through_the_chat_and_their_answers_are_written_down() {
    let dir = tempfile::tempdir().unwrap();
    let questions = dir.path().join("questions.json");
    std::fs::write(&questions, serde_json::json!([
        {"id": 1, "question": "When?", "expected": "In May."},
        {"id": 2, "question": "About this?", "expected": "Boris.", "open": "Boris.md"},
    ]).to_string()).unwrap();
    let out = dir.path().join("answers.md");
    let call = serde_json::json!({"type": "function_call", "call_id": "c1", "name": "search", "arguments": "{\"query\":\"radar\"}"});
    let script = chat_script(dir.path(), &[chat_reply(&[call]), chat_reply(&[chat_message("In May.")]), chat_reply(&[chat_message("It is Boris.")])]);
    let (_vault, out_text) = probe_notes(
        &[("Boris.md", "# Boris"), ("Radar.md", "# Radar\n\nIn May.")],
        &[("KNOTES_PROBE_CHAT", &script), ("KNOTES_CHAT_EVAL", questions.to_str().unwrap()), ("KNOTES_CHAT_EVAL_OUT", out.to_str().unwrap()), ("KNOTES_PROBE_DELAY_MS", "2500")],
    );
    let answers = std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("{e}: {out_text}"));
    assert!(answers.starts_with("# Перевірка ШІ: 2 питань, модель Probe Model"), "{answers}");
    assert!(answers.contains("## 1. When?") && answers.contains("**Інструменти (1):** search {\"query\":\"radar\"}") && answers.contains("In May.\n"), "{answers}");
    assert!(answers.contains("## 2. About this?\n\nВідкрито: Boris.md") && answers.contains("It is Boris."), "{answers}");
    assert!(answers.ends_with("Готово: 2 з 2\n"), "{answers}");
    assert!(out_text.contains("chat conversations: Нова розмова | selected 0"), "the questions are not kept as conversations: {out_text}");
}

/// A vault with two spaces: Work (Робота) and Home (Дім), and a note in neither.
fn spaced_vault() -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in [("Work/a.md", "a"), ("Work/Radar/b.md", "b #radar"), ("Home/c.md", "c"), ("top.md", "top")] {
        let path = vault.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::write(vault.path().join(".knotes.json"), r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}, {"id": "home", "name": "Дім", "color": "green"}]}"#).unwrap();
    std::fs::write(vault.path().join("Work/.knotes.json"), r#"{"space": "work"}"#).unwrap();
    std::fs::write(vault.path().join("Home/.knotes.json"), r#"{"space": "home"}"#).unwrap();
    vault
}

#[test]
fn a_space_keeps_the_sidebar_and_the_list_to_its_folders_and_is_remembered() {
    let (vault, home) = (spaced_vault(), tempfile::tempdir().unwrap());
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_SPACE", "work")]);
    let line = |name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("no {name}: {out}")).to_string();
    assert_eq!(line("space: "), "Робота | spaces: Робота, Дім");
    assert_eq!(line("space-menu: "), "Уся база | ✓ Робота | Дім | — | Налаштувати простори…", "chosen through the toolbar's menu");
    assert_eq!(line("sidebar: "), "Усі нотатки (2) | Без тегів (1) | · | Work (1) | · | Решта бази (2)", "the space's folders (no inbox outside it), then the rest of the vault");
    let list = line("list: ");
    let rest = list.find("[Решта бази]").unwrap_or_else(|| panic!("{list}"));
    let (space, others) = list.split_at(rest);
    assert!(space.contains("a ~") && space.contains("b ~") && others.contains("c ~") && others.contains("top ~"), "the space's notes, then the rest of the vault's: {list}");
    let again = probe_env(&[vault.path()], Some(home.path()), &[]);
    assert!(again.contains("space: Робота |"), "the window's space comes back: {again}");
    let whole = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_SPACE", "")]);
    assert!(whole.contains("space: — |"), "the whole vault: {whole}");
    assert!(whole.contains("Усі нотатки (4)"), "{whole}");
}

#[test]
fn a_vault_without_spaces_offers_itself_whole_and_the_spaces_settings() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    let out = probe_with(&[vault.path()], None);
    assert!(out.contains("space-menu: ✓ Уся база | Налаштувати простори…\n"), "{out}");
    assert!(out.contains("space: — | spaces: \n") || out.lines().any(|l| l == "space: — | spaces: "), "{out}");
}

#[test]
fn a_folder_s_menu_puts_it_in_a_space_or_a_new_one_writing_only_what_differs_from_its_parent() {
    let vault = spaced_vault();
    let rules = |folder: &str| std::fs::read_to_string(vault.path().join(folder).join(".knotes.json")).ok().map(|t| serde_json::from_str::<serde_json::Value>(&t).unwrap());
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-space=Home=work")]);
    assert!(out.contains("sidebar-space-menu: Робота | ✓ Дім | Без простору | У всіх просторах | — | Новий простір…"), "{out}");
    assert_eq!(rules("Home"), Some(serde_json::json!({"space": "work"})));
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-space=Work/Radar=new"), ("KNOTES_PROBE_SPACE_NAME", "Проєкт Радар")]);
    let root = rules("").unwrap();
    assert_eq!(root["spaces"][2], serde_json::json!({"id": "proiekt-radar", "name": "Проєкт Радар", "color": "purple"}), "the next colour no space has");
    assert_eq!(rules("Work/Radar"), Some(serde_json::json!({"space": "proiekt-radar"})));
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-space=Work/Radar=work")]);
    assert_eq!(rules("Work/Radar"), Some(serde_json::json!({})), "its parent's space: the key goes");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-space=Home=none")]);
    assert_eq!(rules("Home"), Some(serde_json::json!({})), "a top folder in none: the root gives none already");
}

#[test]
fn files_in_spaces_show_their_space_s_dot_in_the_whole_vault() {
    let vault = spaced_vault();
    let out = probe_with(&[vault.path()], None);
    let marks = out.lines().find_map(|l| l.strip_prefix("list-marks: ")).unwrap_or_else(|| panic!("{out}"));
    let mut marks: Vec<&str> = marks.split(" | ").collect();
    marks.sort();
    assert_eq!(marks, ["a • Робота", "b • Робота", "c • Дім"], "top.md, in no space, has none");
}

#[test]
fn a_folder_shows_the_dot_of_the_space_it_sets_and_a_tab_of_another_space_its_own() {
    let vault = spaced_vault();
    let out = probe_with(&[vault.path()], None);
    assert!(out.contains("sidebar-marks: Home • Дім | Work • Робота"), "{out}");
    // Tabs opened in the whole vault come back in the space: the one of another space has
    // that space's dot, the space's own none; and a tab of a note kept from the AI its mark.
    std::fs::write(vault.path().join("Work/secret.md"), "#no-ai\n\nтаємне").unwrap();
    let home = tempfile::tempdir().unwrap();
    probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Home/c.md"), ("KNOTES_PROBE_NEW_TAB", "Work/a.md")]);
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_NEW_TAB", "Work/secret.md")]);
    assert!(out.contains("tab-marks: c • Дім | secret • no-ai\n"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_SPACE", "")]);
    assert!(out.contains("tab-marks: c • Дім | a • Робота | secret • Робота, no-ai\n"), "in the whole vault every space's: {out}");
    // The vault's search marks what it finds the same way.
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_VAULT_SEARCH", "c")]);
    let found = out.lines().find_map(|l| l.strip_prefix("vault-search: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(found.contains("Home/c.md • Дім") && found.contains("Work/secret.md • no-ai") && !found.contains("Робота"), "{found}");
}

#[test]
fn in_a_space_the_rest_of_the_vault_s_row_and_folders_list_the_notes_outside_it() {
    let vault = spaced_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_SELECT", "rest")]);
    assert!(out.contains("title: Решта бази"), "{out}");
    let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(list.contains("c ~") && list.contains("top ~") && !list.contains("a ~") && !list.contains("[Решта бази]"), "the notes outside the space, no header: {list}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_SELECT", "f:Home")]);
    let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(list.contains("c ~") && !list.contains("[Решта бази]"), "a folder of the rest lists its notes: {list}");
    let tags = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_SIDEBAR", "tags")]);
    assert!(tags.contains("sidebar: Усі нотатки (2) | Без тегів (1) | · | radar (1) | · | Решта бази (2)"), "{tags}");
}

#[test]
fn an_empty_folder_shows_in_the_sidebar() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir(vault.path().join("Empty")).unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    let out = probe_with(&[vault.path()], None);
    assert!(out.contains("| · | Empty (0)"), "{out}");
}

#[test]
fn a_new_folder_is_made_from_a_folder_s_menu_or_the_sidebar_s_and_named_in_place() {
    let vault = spaced_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "new-folder=Work"), ("KNOTES_PROBE_FOLDER_NAME", "Звіти")]);
    assert!(out.contains("sidebar-menu-items: Перейменувати | Нова підтека | Додати зірочку | Сховати від ШІ | Видалити | Простір"), "{out}");
    assert!(vault.path().join("Work/Звіти").is_dir() && !vault.path().join("Work/Нова тека").exists(), "made and renamed: {out}");
    // At the top of a space: the new folder joins it.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_ACTION", "new-folder=")]);
    assert!(out.contains("sidebar-renaming: Нова тека"), "named in place: {out}");
    let rules = std::fs::read_to_string(vault.path().join("Нова тека/.knotes.json")).unwrap_or_else(|_| panic!("{out}"));
    assert_eq!(serde_json::from_str::<serde_json::Value>(&rules).unwrap(), serde_json::json!({"space": "work"}));
}

#[test]
fn in_a_space_a_new_note_goes_to_one_of_its_folders() {
    let vault = spaced_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_ACTION", "new")]);
    assert!(vault.path().join("Work/Без назви.md").exists(), "the inbox is not the space's: its folder: {out}");
}

#[test]
fn the_settings_spaces_tab_lists_the_vault_s_spaces_and_edits_them() {
    let vault = spaced_vault();
    let root = || serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(vault.path().join(".knotes.json")).unwrap()).unwrap()["spaces"].clone();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "spaces")]);
    let vault_name = vault.path().file_name().unwrap().to_string_lossy().to_string();
    assert!(out.contains(&format!("settings-spaces: {vault_name} | Робота (blue) | Дім (green)")), "{out}");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "spaces"), ("KNOTES_PROBE_SETTINGS_SPACES", "rename:0=Офіс")]);
    assert_eq!(root()[0], serde_json::json!({"id": "work", "name": "Офіс", "color": "blue"}), "renamed, its id kept");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "spaces"), ("KNOTES_PROBE_SETTINGS_SPACES", "color:1=red")]);
    assert_eq!(root()[1]["color"], "red");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "spaces"), ("KNOTES_PROBE_SETTINGS_SPACES", "color:1=#ff8800")]);
    assert_eq!(root()[1]["color"], "#ff8800", "one's own, from the colour panel");
    assert!(out.contains("Дім (#ff8800)") && out.contains("settings-space-colors: Синій | Свій колір"), "shown first in its row's menu: {out}");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "spaces"), ("KNOTES_PROBE_SETTINGS_SPACES", "add")]);
    assert_eq!(root()[2], serde_json::json!({"id": "novyi-prostir", "name": "Новий простір", "color": "purple"}));
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "spaces"), ("KNOTES_PROBE_SETTINGS_SPACES", "delete:0")]);
    assert_eq!(root().as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect::<Vec<_>>(), ["home", "novyi-prostir"]);
}

#[test]
fn the_chat_is_told_the_window_s_space() {
    let vault = spaced_vault();
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &[chat_reply(&[chat_message("ok")])]);
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_CHAT", &script), ("KNOTES_PROBE_CHAT_SEND", "що в планах?"), ("KNOTES_PROBE_DELAY_MS", "2500")]);
    let told = out.lines().find_map(|l| l.strip_prefix("chat-told: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(told.contains("«Робота»"), "{told}");
}

#[test]
fn a_folder_put_in_every_space_from_its_menu_shows_in_each() {
    let vault = spaced_vault();
    std::fs::create_dir_all(vault.path().join("Контакти")).unwrap();
    std::fs::write(vault.path().join("Контакти/Анна.md"), "Анна").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-space=Контакти=shared")]);
    assert!(out.contains("sidebar-space-menu: Робота | Дім | ✓ Без простору | У всіх просторах | — | Новий простір…"), "{out}");
    let rules = std::fs::read_to_string(vault.path().join("Контакти/.knotes.json")).unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(&rules).unwrap(), serde_json::json!({"space": "*"}));
    for space in ["work", "home"] {
        let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SPACE", space)]);
        let sidebar = out.lines().find_map(|l| l.strip_prefix("sidebar: ")).unwrap_or_else(|| panic!("{out}"));
        let rest = sidebar.find("Решта бази").unwrap();
        assert!(sidebar[..rest].contains("Контакти (1)"), "{space}: among its own folders: {sidebar}");
    }
}

/// The entries in a vault's recently deleted.
fn deleted_entries(vault: &std::path::Path) -> usize {
    std::fs::read_dir(vault.join(".knotes/trash")).map_or(0, |d| d.count())
}

#[test]
fn a_new_note_with_a_first_line_typed_is_deleted_under_the_name_it_takes() {
    // A new note, one line typed (no line break yet: it keeps «Без назви» until it is left).
    // Deleted, it is left first and takes its line's name; it went to the deleted under that
    // name or the delete failed, looking for the old one.
    let (vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "new"), ("KNOTES_PROBE_TYPE", "Ідея"), ("KNOTES_PROBE_DELETE_OPEN", "1")]);
    assert!(!out.contains("failed: "), "{out}");
    let left: Vec<String> = std::fs::read_dir(vault.path().join("Вхідні")).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    assert!(left.iter().all(|name| !name.ends_with(".md")), "no note left behind: {left:?} {out}");
    let trash = vault.path().join(".knotes/trash");
    let deleted = std::fs::read_dir(&trash).unwrap().flatten().count();
    assert_eq!(deleted, 1, "{out}");
}

#[test]
fn a_note_deleted_from_its_menu_goes_to_recently_deleted_and_comes_back_from_there() {
    let vault = spaced_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Work/a.md"), ("KNOTES_PROBE_ACTION", "list-delete")]);
    assert!(out.contains("list-menu-items: Відкрити в новій вкладці | Перейменувати | Перемістити в… | Додати зірочку | Сховати від ШІ | — | Видалити"), "{out}");
    assert!(!vault.path().join("Work/a.md").exists() && deleted_entries(vault.path()) == 1, "{out}");
    assert!(out.contains("Нещодавно видалені (1)"), "a row for them: {out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "deleted"), ("KNOTES_PROBE_ACTION", "list-restore")]);
    assert!(out.contains("list-menu-items: Відновити | Видалити назавжди | — | Очистити нещодавно видалені…"), "{out}");
    assert!(out.contains("title: Усі нотатки"), "none left: their row goes, and the list with it: {out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Work/a.md")).unwrap(), "a", "back where it was");
    assert_eq!(deleted_entries(vault.path()), 0);
}

#[test]
fn a_folder_deleted_from_its_menu_goes_whole_and_the_deleted_are_listed_and_emptied() {
    let vault = spaced_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-delete=Home")]);
    assert!(out.contains("sidebar-menu-items: Перейменувати | Нова підтека | Додати зірочку | Сховати від ШІ | Видалити | Простір"), "{out}");
    assert!(!vault.path().join("Home").exists() && deleted_entries(vault.path()) == 1, "{out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "deleted")]);
    let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap_or_else(|| panic!("{out}"));
    assert!(list.starts_with("Home ~ ") && list.contains("тека · 1 нотатка"), "{list}");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "deleted"), ("KNOTES_PROBE_ACTION", "list-empty")]);
    assert_eq!(deleted_entries(vault.path()), 0, "gone for good");
}

/// The attachments' window lists the vault's files that are no notes with the links to each,
/// those with no link, and the links to images that are not there; files chosen there are
/// deleted into the recently deleted.
#[test]
fn the_attachments_window_lists_files_unlinked_ones_and_missing_ones_and_deletes() {
    let notes = [("a.md", "![[pic.png]] and ![[gone.png]]"), ("Dir/b.md", "![[pic.png]]")];
    let files: [(&str, &[u8]); 3] = [("pic.png", PIC), ("Dir/lone.png", PIC), ("extra.png", PIC)];
    let line = |out: &str| out.lines().find_map(|l| l.strip_prefix("attachments: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    let (_vault, out) = probe_images(&notes, &files, &[("KNOTES_PROBE_ATTACHMENTS", "all")]);
    let all = line(&out);
    assert!(all.starts_with("Усі · 3 | Без посилань · 2 | Немає файлу · 1 || "), "{all}");
    assert!(all.contains("lone.png — Dir · ") && all.contains("немає посилань | extra.png — корінь бази"), "by path, each with its folder: {all}");
    assert!(all.contains("pic.png — корінь бази · ") && all.contains(" · 2 посилання || 3 вкладення · "), "{all}");
    let (_vault, out) = probe_images(&notes, &files, &[("KNOTES_PROBE_ATTACHMENTS", "missing")]);
    assert!(line(&out).contains("|| gone.png — посилання в нотатці «a» || 1 посилання без файлу"), "{}", line(&out));
    // The two with no link, chosen and deleted: off the list, kept in the recently deleted.
    let (vault, out) = probe_images(&notes, &files, &[("KNOTES_PROBE_ATTACHMENTS", "unlinked"), ("KNOTES_PROBE_ATTACHMENTS_ACT", "0,1")]);
    assert!(line(&out).starts_with("Усі · 1 | Без посилань · 0 | Немає файлу · 1 ||  || 0 вкладень"), "{}", line(&out));
    assert!(!vault.path().join("Dir/lone.png").exists() && !vault.path().join("extra.png").exists() && vault.path().join("pic.png").exists(), "{out}");
    assert_eq!(deleted_entries(vault.path()), 2);
}

/// The pointer over an image's row shows its picture; a file deleted with its links leaves the
/// notes without them (the one open too), a link alone on its line with its line.
#[test]
fn an_attachment_is_previewed_under_the_pointer_and_deleted_with_its_links() {
    let notes = [("a.md", "Before\n\n![[pic.png]]\n\nAfter"), ("b.md", "See ![[pic.png]] here")];
    let files: [(&str, &[u8]); 1] = [("pic.png", PIC)];
    let env = [("KNOTES_PROBE_ATTACHMENTS", "all"), ("KNOTES_PROBE_ATTACHMENTS_HOVER", "0"), ("KNOTES_PROBE_OPEN", "a.md")];
    let (vault, out) = probe_images(&notes, &files, &env);
    assert!(out.contains("attachment-preview: pic.png 120x80"), "{out}");
    assert!(vault.path().join("pic.png").exists());
    let env = [("KNOTES_PROBE_ATTACHMENTS", "all"), ("KNOTES_PROBE_ATTACHMENTS_ACT", "0"), ("KNOTES_PROBE_ATTACHMENTS_LINKS", "1"), ("KNOTES_PROBE_OPEN", "a.md")];
    let (vault, out) = probe_images(&notes, &files, &env);
    assert!(!vault.path().join("pic.png").exists() && deleted_entries(vault.path()) == 1, "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "Before\n\nAfter");
    assert_eq!(std::fs::read_to_string(vault.path().join("b.md")).unwrap(), "See here");
    assert!(out.contains("note: a.md | Before⏎⏎After"), "the note open shows it: {out}");
    // Without the links asked for, the notes stay as they were.
    let env = [("KNOTES_PROBE_ATTACHMENTS", "all"), ("KNOTES_PROBE_ATTACHMENTS_ACT", "0")];
    let (vault, _) = probe_images(&notes, &files, &env);
    assert_eq!(std::fs::read_to_string(vault.path().join("b.md")).unwrap(), "See ![[pic.png]] here");
}

/// Over the recently deleted, while there are some, a button empties them; over another list
/// and over none deleted there is no such button.
#[test]
fn a_button_over_the_recently_deleted_empties_them() {
    let vault = spaced_vault();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Work/a.md"), ("KNOTES_PROBE_ACTION", "list-delete")]);
    assert_eq!(deleted_entries(vault.path()), 1);
    assert!(probe_env(&[vault.path()], None, &[]).contains("empty-deleted: hidden"), "not over the notes");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "deleted"), ("KNOTES_PROBE_ACTION", "empty-button")]);
    assert_eq!(deleted_entries(vault.path()), 0, "gone for good: {out}");
    assert!(out.contains("empty-deleted: hidden"), "nothing left to empty: {out}");
}

#[test]
fn a_deleted_note_pdf_or_folder_chosen_in_recently_deleted_shows_as_it_was_and_is_not_edited() {
    let vault = spaced_vault();
    let line = |out: &str, name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("no {name}: {out}")).to_string();
    let show = || probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "deleted"), ("KNOTES_PROBE_ACTION", "list-show")]);
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Work/a.md"), ("KNOTES_PROBE_ACTION", "list-delete")]);
    let out = show();
    assert_eq!(line(&out, "note-column: "), "note (read only)", "the note, not «no document»: {out}");
    assert_eq!(line(&out, "text: "), "a");
    assert!(!vault.path().join("Work/a.md").exists(), "shown, not restored, nor written back: {out}");
    std::fs::create_dir_all(vault.path().join("Docs")).unwrap();
    std::fs::write(vault.path().join("Docs/x.pdf"), pdf_of(2)).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    knotes_core::TrashStore::trash(&knotes_vault::FsVault::new(vault.path()), "Docs/x.pdf", now).unwrap();
    let out = show();
    assert_eq!(line(&out, "note-column: "), "pdf", "the newest first: the PDF, in its viewer: {out}");
    assert_eq!(line(&out, "pdf-view: "), "Docs/x.pdf 1/2");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-delete=Home")]);
    let out = show();
    assert_eq!(line(&out, "note-column: "), "empty: Тека «Home» · 1 нотатка. Відновіть її, щоб відкрити.", "{out}");
}

#[test]
fn a_note_come_back_to_at_its_start_keeps_the_margin_over_its_first_line() {
    // Put back where it was, its first line went to the view's very top, the margin over it
    // scrolled away; a short note has nowhere to scroll at all.
    let home = tempfile::tempdir().unwrap();
    let notes = [("a.md", "one\ntwo\nthree\n"), ("b.md", "b")];
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_OPEN", "a.md,b.md,a.md")], home.path());
    assert!(out.contains("note-visible: 0.0 "), "back from another note: {out}");
}

#[test]
fn typing_at_a_note_s_start_keeps_the_margin_over_its_first_line() {
    // AppKit scrolled the line typed in to the view's very top: the margin over it was gone.
    for (text, typed_at) in [("", "0,0"), ("one\ntwo\n", "0,0"), ("one\ntwo\n", "8,0")] {
        let (_vault, out) = probe_notes(&[("a.md", text)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "x"), ("KNOTES_PROBE_RANGE", typed_at)]);
        assert!(out.contains("note-visible: 0.0 "), "{text:?} typed in at {typed_at}: {out}");
    }
}

#[test]
fn the_calendar_marks_the_days_with_a_note_and_a_click_on_a_day_opens_its_note_or_makes_it() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Old")).unwrap();
    for (path, text) in [("Old/2026-11-03.md", "third"), ("2026-11-21.md", "x"), ("2026-10-31.md", "oct"), ("a.md", "a")] {
        std::fs::write(vault.path().join(path), text).unwrap();
    }
    // A HOME whose settings show the calendar in the right panel.
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("Library/Application Support/Knotes");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings-v2.json"), r#"{"panel": "calendar"}"#).unwrap();
    // Noon of 22 November 2026 in Greenwich: that day in every time zone but the farthest.
    let probe = |clicks: &str| probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_NOW", "1795348800"), ("KNOTES_PROBE_CALENDAR", clicks)]);
    let out = probe("");
    assert!(out.contains("panel: calendar, shows calendar\n") && out.contains("calendar: Листопад 2026 | today 22 | notes 3 21\n"), "the notes named by a day, wherever they lie: {out}");
    let out = probe("day=3");
    assert!(out.contains("path-bar: 📕 › Old › 2026-11-03\n"), "the day's note opened: {out}");
    let out = probe("day=22");
    assert!(out.contains("path-bar: 📕 › Щоденник › 2026-11-22\n"), "made in the folder of daily notes: {out}");
    assert_eq!(read(vault.path(), "Щоденник/2026-11-22.md"), "");
    let out = probe("before");
    assert!(out.contains("calendar: Жовтень 2026 | today — | notes 31\n"), "{out}");
    let out = probe("before; before; today");
    assert!(out.contains("calendar: Листопад 2026 | today 22 | notes 3 21 22\n"), "back to this month, the new note in it: {out}");
    // The folder the vault's root names instead.
    std::fs::write(vault.path().join(".knotes.json"), r#"{"daily": {"folder": "Journal"}}"#).unwrap();
    let out = probe("after; after; day=1");
    assert!(out.contains("calendar: Січень 2027 |") && out.contains("path-bar: 📕 › Journal › 2027-01-01\n"), "{out}");
}

#[test]
fn a_folder_and_a_note_are_starred_from_their_menus_and_listed_in_the_right_panel() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(vault.path().join("Projects")).unwrap();
    std::fs::write(vault.path().join("Projects/Plan.md"), "# Plan").unwrap();
    std::fs::write(vault.path().join("a.md"), "note a").unwrap();
    // A HOME whose settings show the stars in the right panel.
    let home = tempfile::tempdir().unwrap();
    let data = home.path().join("Library/Application Support/Knotes");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings-v2.json"), r#"{"panel": "stars"}"#).unwrap();
    let probe = |env: &[(&str, &str)]| probe_env(&[vault.path()], Some(home.path()), env);
    let out = probe(&[]);
    assert!(out.contains("panel: stars, shows stars\n") && out.contains("stars: \n") && out.contains("stars-empty: Тут з'являться теки й файли із зірочкою."), "{out}");
    let out = probe(&[("KNOTES_PROBE_ACTION", "folder-star=Projects")]);
    assert!(out.contains("sidebar-menu-items: Перейменувати | Нова підтека | Додати зірочку |") && out.contains("stars: Projects/\n") && !out.contains("stars-empty"), "{out}");
    let out = probe(&[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_ACTION", "list-star")]);
    assert!(out.contains("| Перемістити в… | Додати зірочку | Сховати від ШІ |") && out.contains("stars: Projects/ | a\n"), "in the order starred: {out}");
    assert!(out.contains("star-marks: list a | sidebar Projects\n"), "a star after the note's title in the list and after the folder's name: {out}");
    let rules: serde_json::Value = serde_json::from_str(&read(vault.path(), ".knotes.json")).unwrap();
    assert_eq!(rules["stars"], serde_json::json!(["Projects/", "a.md"]), "kept with the vault");
    // A click shows the folder, or opens the file.
    let out = probe(&[("KNOTES_PROBE_STAR_CLICK", "Projects")]);
    assert!(out.contains("title: Projects\n"), "{out}");
    std::fs::write(data.join("settings-v2.json"), r#"{"panel": "stars"}"#).unwrap();
    let out = probe(&[("KNOTES_PROBE_STAR_CLICK", "a")]);
    assert!(out.contains("path-bar: 📕 › a\n"), "{out}");
    // A star follows its note and its folder under their new names.
    let out = probe(&[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_ACTION", "rename=New")]);
    assert!(out.contains("stars: Projects/ | New\n"), "{out}");
    let out = probe(&[("KNOTES_PROBE_ACTION", "folder-rename=Projects=Work")]);
    assert!(out.contains("sidebar-menu-items: Перейменувати | Нова підтека | Прибрати зірочку |") && out.contains("stars: Work/ | New\n"), "{out}");
    // The star at a row's end takes it off; put back by the folder's menu.
    let out = probe(&[("KNOTES_PROBE_STAR_BUTTON", "Work")]);
    assert!(out.contains("stars: New\n") && out.contains("star-marks: list New | sidebar \n"), "{out}");
    let out = probe(&[("KNOTES_PROBE_ACTION", "folder-star=Work")]);
    assert!(out.contains("stars: New | Work/\n"), "{out}");
    // The first row is pressed where the words said with no stars were: they are out of its way.
    let out = probe(&[("KNOTES_PROBE_STAR_CLICK", "New")]);
    assert!(!out.contains("star-click: not on the list") && out.contains("path-bar: 📕 › New\n"), "{out}");
    // What is deleted is listed no more; a star's own menu takes it off.
    std::fs::remove_file(vault.path().join("New.md")).unwrap();
    let out = probe(&[("KNOTES_PROBE_STAR_UNSTAR", "Work")]);
    assert!(out.contains("star-menu-items: Прибрати зірочку\n") && out.contains("stars: \n"), "{out}");
    let rules: serde_json::Value = serde_json::from_str(&read(vault.path(), ".knotes.json")).unwrap();
    assert_eq!(rules["stars"], serde_json::json!(["New.md"]), "a deleted note's star waits for it");
}

#[test]
fn the_settings_text_tab_sets_the_font_the_size_and_the_line_height_of_a_note_and_of_the_chat() {
    let home = tempfile::tempdir().unwrap();
    let script_dir = tempfile::tempdir().unwrap();
    let script = chat_script(script_dir.path(), &[chat_reply(&[chat_message("In May.")])]);
    let notes = [("a.md", "plain text")];
    let shown = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_SETTINGS", "text"), ("KNOTES_PROBE_CHAT", script.as_str()), ("KNOTES_PROBE_CHAT_SEND", "When?"), ("KNOTES_PROBE_DELAY_MS", "2000")];
    let (_vault, out) = probe_notes_at_home(&notes, &shown, home.path());
    assert!(out.contains("settings: Текст |") && out.contains("settings-text: Системний 15 1,0 | Системний 14 1,0\n"), "the app's own: {out}");
    assert!(out.contains("note-font: .AppleSystemUIFont 15 +0.0\n") && out.contains("chat-font: .AppleSystemUIFont 14 +1.5\n"), "{out}");
    // Set while the note and the conversation are shown: both take it at once.
    let set = [shown.as_slice(), &[("KNOTES_PROBE_SETTINGS_TEXT", "note font Georgia; note size 18; note line 150; chat font Menlo; chat size 12; chat line 130")]].concat();
    let (_vault, out) = probe_notes_at_home(&notes, &set, home.path());
    assert!(out.contains("settings-text: Georgia 18 1,5 | Menlo 12 1,3\n"), "{out}");
    assert!(out.contains("note-font: Georgia 18 +10.2\n") && out.contains("chat-font: Menlo 12 +5.7\n"), "{out}");
    // The room is between the lines, not in them: the caret stays as tall as the text.
    assert!(out.contains("note-caret: 21.0\n"), "Georgia's own height at 18 points, not half as tall again: {out}");
    let (_vault, out) = probe_notes_at_home(&notes, &shown, home.path());
    assert!(out.contains("settings-text: Georgia 18 1,5 | Menlo 12 1,3\n") && out.contains("note-font: Georgia 18 +10.2\n") && out.contains("chat-font: Menlo 12 +5.7\n"), "kept: {out}");
    // A size out of range is brought into it; words are no size.
    let (_vault, out) = probe_notes_at_home(&notes, &[("KNOTES_PROBE_SETTINGS", "text"), ("KNOTES_PROBE_SETTINGS_TEXT", "note size 400; chat size big; note font Системний")], home.path());
    assert!(out.contains("settings-text: Системний 36 1,5 | Menlo 12 1,3\n"), "{out}");
}

#[test]
fn the_settings_deleted_tab_sets_the_days_the_vault_keeps_them() {
    let vault = spaced_vault();
    let name = vault.path().file_name().unwrap().to_string_lossy().to_string();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "trash")]);
    assert!(out.contains(&format!("settings-trash: {name} | 30")), "30 by default: {out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SETTINGS", "trash"), ("KNOTES_PROBE_SETTINGS_TRASH_DAYS", "14")]);
    let root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(vault.path().join(".knotes.json")).unwrap()).unwrap();
    assert_eq!(root["trash"]["days"], 14, "{out}");
    assert_eq!(root["spaces"].as_array().map(Vec::len), Some(2), "the rest of the file kept");
    assert!(out.contains(&format!("settings-trash: {name} | 14")), "{out}");
}

#[test]
fn a_pdf_renamed_from_the_list_stays_a_pdf() {
    // A PDF is listed among a search's results with a note's menu (ADR 0022): renaming it there
    // made `Contract.md` of it.
    let notes = [("a.md", "see [[doc.pdf]]")];
    let (vault, out) = probe_images(&notes, &[("Docs/doc.pdf", PDF)], &[("KNOTES_PROBE_ACTION", "list-rename-path=Docs/doc.pdf=Contract")]);
    assert!(std::fs::read(vault.path().join("Docs/Contract.pdf")).unwrap().starts_with(b"%PDF"), "{out}");
    assert!(!vault.path().join("Docs/Contract.md").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "see [[Contract.pdf]]");
}

#[test]
fn a_pdf_moved_from_the_list_keeps_its_name() {
    // Dropped on a folder (or moved through the row's menu) it became `doc.pdf.md`.
    let notes = [("a.md", "see [[Docs/doc.pdf]]"), ("Archive/b.md", "b")];
    let (vault, out) = probe_images(&notes, &[("Docs/doc.pdf", PDF)], &[("KNOTES_PROBE_ACTION", "drop-path=Docs/doc.pdf=Archive")]);
    assert!(std::fs::read(vault.path().join("Archive/doc.pdf")).unwrap().starts_with(b"%PDF"), "{out}");
    assert!(!vault.path().join("Archive/doc.pdf.md").exists(), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("a.md")).unwrap(), "see [[Archive/doc.pdf]]");
}

/// A vault whose `Locked` folder cannot be written, the probe run on it with the note
/// `Locked/n.md` open and typed in; the folder writable again before the vault goes.
fn probe_locked(env: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
    let vault = tempfile::tempdir().unwrap();
    std::fs::create_dir(vault.path().join("Locked")).unwrap();
    std::fs::write(vault.path().join("Locked/n.md"), "old text").unwrap();
    std::fs::write(vault.path().join("other.md"), "other").unwrap();
    let locked = vault.path().join("Locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut env = env.to_vec();
    env.push(("KNOTES_PROBE_TYPE", " NEW"));
    if !env.iter().any(|(k, _)| *k == "KNOTES_PROBE_OPEN") {
        env.push(("KNOTES_PROBE_OPEN", "Locked/n.md"));
    }
    let out = probe_env(&[vault.path()], None, &env);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    (vault, out)
}

#[test]
fn a_save_that_fails_is_told() {
    let (vault, out) = probe_locked(&[("KNOTES_PROBE_ACTION", "save")]);
    assert!(out.contains("save-failed: Locked/n.md: Permission denied"), "{out}");
    assert!(out.contains("note: Locked/n.md | old text NEW"), "the edits stay in the window: {out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Locked/n.md")).unwrap(), "old text");
}

#[test]
fn a_window_whose_edits_cannot_be_written_stays_open() {
    let (_vault, out) = probe_locked(&[("KNOTES_PROBE_ACTION", "close")]);
    assert!(out.contains("unsaved-question: Locked/n.md: Permission denied"), "{out}");
    assert!(out.contains("window stays open"), "{out}");
}

#[test]
fn a_window_closes_without_edits_that_cannot_be_written_when_the_user_says_so() {
    let (vault, out) = probe_locked(&[("KNOTES_PROBE_ACTION", "close"), ("KNOTES_PROBE_ANSWER", "discard")]);
    assert!(out.contains("unsaved-question: Locked/n.md"), "{out}");
    assert!(!out.contains("window stays open"), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Locked/n.md")).unwrap(), "old text");
}

#[test]
fn a_note_whose_edits_cannot_be_written_stays_open_when_another_is_chosen() {
    let (_vault, out) = probe_locked(&[("KNOTES_PROBE_OPEN", "Locked/n.md,other.md")]);
    assert!(out.contains("unsaved-question: Locked/n.md"), "{out}");
    assert!(out.contains("note: Locked/n.md | old text NEW"), "{out}");
    // Quitting asks again and, kept, is called off.
    assert!(out.contains("quit called off"), "{out}");
}

#[test]
fn a_note_not_in_utf8_opens_read_only_and_is_never_written() {
    // "Привіт, світ" in Windows-1251: read as UTF-8, a key typed wrote it back as U+FFFD.
    let cp1251: &[u8] = &[0xcf, 0xf0, 0xe8, 0xe2, 0xb3, 0xf2, 0x2c, 0x20, 0xf1, 0xe2, 0xb3, 0xf2];
    let (vault, out) = probe_images(&[("a.md", "a")], &[("old.md", cp1251)], &[("KNOTES_PROBE_OPEN", "old.md"), ("KNOTES_PROBE_TYPE", "!"), ("KNOTES_PROBE_ACTION", "save")]);
    assert!(out.contains("read-only: old.md: not UTF-8"), "{out}");
    assert!(out.contains("note-column: note (read only)"), "{out}");
    assert_eq!(std::fs::read(vault.path().join("old.md")).unwrap(), cp1251);
}

#[test]
fn a_note_not_in_utf8_keeps_its_bytes_when_a_note_it_links_to_is_renamed() {
    // Its link is not rewritten: written back, the note would lose its other bytes.
    let mut linking = b"[[Old]] ".to_vec();
    linking.extend([0xcf, 0xf0, 0xe8, 0xe2, 0xb3, 0xf2]);
    let (vault, out) = probe_images(&[("Old.md", "old")], &[("b.md", &linking)], &[("KNOTES_PROBE_OPEN", "Old.md"), ("KNOTES_PROBE_ACTION", "rename=New")]);
    assert!(vault.path().join("New.md").exists(), "{out}");
    assert_eq!(std::fs::read(vault.path().join("b.md")).unwrap(), linking);
}

#[test]
fn a_sort_order_chosen_keeps_the_rest_of_the_folder_s_rules_file() {
    let rules = r#"{"space": "work", "ai": false, "list": {"group": false}}"#;
    let (vault, out) = probe_notes(&[("Work/a.md", "a"), ("Work/.knotes.json", rules)], &[("KNOTES_PROBE_SELECT", "f:Work"), ("KNOTES_PROBE_ACTION", "sort=title")]);
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(vault.path().join("Work/.knotes.json")).unwrap()).unwrap();
    assert_eq!(saved, serde_json::json!({"space": "work", "ai": false, "list": {"group": false, "sort": "title"}}), "{out}");
}

#[test]
fn a_sort_order_chosen_leaves_a_rules_file_that_does_not_parse_as_it_is() {
    // A comma too many, as typed by hand: the snapshot ignores the file, and the choice wrote
    // `{"list": …}` over it, taking its space and `"ai": false` away.
    let rules = "{\n  \"space\": \"work\",\n  \"ai\": false,\n}\n";
    let (vault, out) = probe_notes(&[("Work/a.md", "a"), ("Work/.knotes.json", rules)], &[("KNOTES_PROBE_SELECT", "f:Work"), ("KNOTES_PROBE_ACTION", "sort=title")]);
    assert!(out.contains("rules-not-written: Work/.knotes.json: .knotes.json: trailing comma"), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Work/.knotes.json")).unwrap(), rules);
}

/// The files in `dir` named as conflict copies of the note `stem` (`stem (конфлікт …).md`).
fn conflict_copies(dir: &std::path::Path, stem: &str) -> Vec<String> {
    let prefix = format!("{stem} (конфлікт ");
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with(&prefix) && n.ends_with(").md")).collect()
}

#[test]
fn a_new_note_named_after_its_first_line_keeps_its_tab() {
    let home = tempfile::tempdir().unwrap();
    let (_vault, out) = probe_notes_at_home(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "new"), ("KNOTES_PROBE_TYPE", "Мій заголовок\nтекст")], home.path());
    // The tabs as the app left them on quitting, beside the vault's index.
    let vaults = home.path().join("Library/Application Support/Knotes/vaults");
    let file = std::fs::read_dir(vaults).unwrap().map(|e| e.unwrap().path().join("tabs.json")).find(|p| p.exists()).expect("tabs.json");
    let tabs = std::fs::read_to_string(file).unwrap();
    assert!(tabs.contains("Вхідні/Мій заголовок.md"), "{tabs}\n{out}");
    assert!(!tabs.contains("Без назви"), "{tabs}");
}

#[test]
fn an_open_note_deleted_outside_closes_and_its_unsaved_edits_go_beside_it() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    std::fs::write(vault.path().join("other.md"), "o").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_TYPE", "!"), ("KNOTES_PROBE_DELETE", "1"), ("KNOTES_PROBE_DELAY_MS", "1500")]);
    assert!(!vault.path().join("n.md").exists(), "not made again where it was: {out}");
    let copies = conflict_copies(vault.path(), "n");
    assert_eq!(copies.len(), 1, "{copies:?} {out}");
    assert_eq!(read(vault.path(), &copies[0]), "hello!");
    assert!(out.contains(&format!("vanished-notice: {}", copies[0])), "{out}");
    assert!(out.contains("note-column: empty"), "{out}");
}

#[test]
fn an_open_note_deleted_outside_without_edits_closes_and_comes_back_nowhere() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("n.md"), "hello").unwrap();
    std::fs::write(vault.path().join("other.md"), "o").unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_DELETE", "1"), ("KNOTES_PROBE_DELAY_MS", "1500")]);
    assert!(!vault.path().join("n.md").exists(), "{out}");
    assert!(conflict_copies(vault.path(), "n").is_empty(), "{out}");
    assert!(out.contains("note-column: empty"), "{out}");
}

#[test]
fn a_note_that_cannot_be_read_opens_read_only_and_is_never_written() {
    use std::os::unix::fs::PermissionsExt;
    // Unreadable (or offline in iCloud), it opened empty and editable: a key typed replaced it.
    let vault = tempfile::tempdir().unwrap();
    let file = vault.path().join("n.md");
    std::fs::write(&file, "secret").unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_TYPE", "typed"), ("KNOTES_PROBE_ACTION", "save")]);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(out.contains("unreadable: n.md: Permission denied"), "{out}");
    assert!(out.contains("note-column: note (read only)"), "{out}");
    assert_eq!(read(vault.path(), "n.md"), "secret");
}

#[test]
fn an_embedded_note_changed_outside_while_edited_keeps_both_versions() {
    let notes = [("a.md", "Top\n\n![[b]]\n\nend"), ("b.md", "Body.")];
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end")];
    let (vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_EMBED_TYPE", " More."), ("KNOTES_PROBE_EMBED_OUTSIDE", "Changed outside.")]);
    assert_eq!(read(vault.path(), "b.md"), "Body. More.", "the edits keep the note's name: {out}");
    let copies = conflict_copies(vault.path(), "b");
    assert_eq!(copies.len(), 1, "{copies:?} {out}");
    assert_eq!(read(vault.path(), &copies[0]), "Changed outside.");
    assert!(out.contains(&format!("conflict-notice: {}", copies[0])), "{out}");
}

#[test]
fn an_embedded_section_written_back_keeps_a_change_made_outside_elsewhere_in_its_note() {
    let notes = [("a.md", "![[b#Part]]"), ("b.md", "# B\n\n## Part\n\nPart text.\n\n## Next\n\nN.\n")];
    let outside = "# B changed\n\n## Part\n\nPart text.\n\n## Next\n\nN.\n";
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_EMBED_TYPE", " More."), ("KNOTES_PROBE_EMBED_OUTSIDE", outside)]);
    assert_eq!(read(vault.path(), "b.md"), "# B changed\n\n## Part\n\nPart text. More.\n\n## Next\n\nN.\n", "{out}");
    assert!(conflict_copies(vault.path(), "b").is_empty(), "no conflict outside the part: {out}");
}

#[test]
fn an_embedded_note_deleted_outside_while_edited_comes_back_only_as_a_copy() {
    let notes = [("a.md", "Top\n\n![[b]]\n\nend"), ("b.md", "Body.")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_EMBED_TYPE", " More."), ("KNOTES_PROBE_EMBED_GONE", "1")]);
    assert!(!vault.path().join("b.md").exists(), "not made again where it was: {out}");
    let copies = conflict_copies(vault.path(), "b");
    assert_eq!(copies.len(), 1, "{copies:?} {out}");
    assert_eq!(read(vault.path(), &copies[0]), "Body. More.");
    assert!(out.contains(&format!("vanished-notice: {}", copies[0])), "{out}");
}

#[test]
fn edits_in_an_embed_whose_line_is_deleted_are_still_written() {
    // "![[b]]\n\n" is 8 long after "Top\n\n": deleted before the embed's edits were written,
    // its view went, and they with it.
    let notes = [("a.md", "Top\n\n![[b]]\n\nend"), ("b.md", "Body.")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_EMBED_TYPE", " More."), ("KNOTES_PROBE_EMBED_CUT", "5,8")]);
    assert_eq!(read(vault.path(), "b.md"), "Body. More.", "{out}");
    assert_eq!(read(vault.path(), "a.md"), "Top\n\nend");
}

#[test]
fn a_link_just_typed_follows_the_note_it_names_when_that_is_renamed() {
    // Written by the rename itself a moment before, the note's new link was not in the index yet:
    // the rename did not see it, and it broke.
    let notes = [("a.md", "draft"), ("Old.md", "old")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", " [[Old]]"), ("KNOTES_PROBE_ACTION", "list-rename-path=Old.md=New")]);
    assert!(vault.path().join("New.md").exists(), "{out}");
    assert!(out.contains("rename-question: 1 1"), "{out}");
    assert_eq!(read(vault.path(), "a.md"), "draft [[New]]");
}

/// A vault whose `Locked` folder (holding `n.md`) cannot be written, the probe run on it; the
/// folder writable again before the vault goes.
fn probe_read_only(env: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
    let vault = tempfile::tempdir().unwrap();
    let locked = vault.path().join("Locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("n.md"), "n").unwrap();
    std::fs::write(vault.path().join("other.md"), "other").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = probe_env(&[vault.path()], None, env);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    (vault, out)
}

#[test]
fn a_note_that_cannot_be_deleted_says_so() {
    // It said nothing: the note simply stayed.
    let (vault, out) = probe_read_only(&[("KNOTES_PROBE_OPEN", "Locked/n.md"), ("KNOTES_PROBE_ACTION", "list-delete")]);
    assert!(out.contains("failed: Не вдалося видалити «n»: Permission denied"), "{out}");
    assert!(vault.path().join("Locked/n.md").exists());
}

#[test]
fn a_note_that_cannot_be_renamed_says_so() {
    let (vault, out) = probe_read_only(&[("KNOTES_PROBE_ACTION", "list-rename-path=Locked/n.md=New")]);
    assert!(out.contains("failed: Не вдалося перейменувати «n»: Permission denied"), "{out}");
    assert!(vault.path().join("Locked/n.md").exists());
}

#[test]
fn a_folder_whose_space_cannot_be_written_says_so() {
    let rules = r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}]}"#;
    use std::os::unix::fs::PermissionsExt;
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join(".knotes.json"), rules).unwrap();
    let locked = vault.path().join("Locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("n.md"), "n").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-space=Locked=work")]);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(out.contains("failed: Не вдалося змінити простір теки «Locked»: Permission denied"), "{out}");
}

#[test]
fn a_vault_with_a_folder_that_cannot_be_read_still_opens() {
    use std::os::unix::fs::PermissionsExt;
    // The walk failed on it, the indexing stopped before the first snapshot, and the window stayed
    // empty without a word.
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    std::fs::write(vault.path().join("b.md"), "b").unwrap();
    let private = vault.path().join("Private");
    std::fs::create_dir(&private).unwrap();
    std::fs::write(private.join("s.md"), "s").unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o000)).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_DELAY_MS", "1000")]);
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(out.contains("subtitle: 2 нотатки"), "{out}");
    assert!(!out.contains("failed:"), "{out}");
}

#[test]
fn a_vault_with_a_rules_file_that_cannot_be_read_still_opens() {
    use std::os::unix::fs::PermissionsExt;
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), "a").unwrap();
    std::fs::create_dir(vault.path().join("Work")).unwrap();
    std::fs::write(vault.path().join("Work/w.md"), "w").unwrap();
    let rules = vault.path().join("Work/.knotes.json");
    std::fs::write(&rules, "{}").unwrap();
    std::fs::set_permissions(&rules, std::fs::Permissions::from_mode(0o000)).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_DELAY_MS", "1000")]);
    std::fs::set_permissions(&rules, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(out.contains("subtitle: 2 нотатки"), "{out}");
    assert!(!out.contains("failed:"), "{out}");
}

#[test]
fn typing_at_the_end_of_the_line_above_a_table_stays_on_that_line() {
    // Mid-edit the table still had its old start: the cursor moved into its first cell after the
    // first key, and the next keys went there (`IntroX` and `| ZYa |`).
    let notes = [("a.md", "Intro\n| a | b |\n| --- | --- |\n| 1 | 2 |\n")];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "XYZ"), ("KNOTES_PROBE_TYPE_CHARS", "1"), ("KNOTES_PROBE_RANGE", "5,0"), ("KNOTES_PROBE_ACTION", "save")]);
    assert_eq!(read(vault.path(), "a.md"), "IntroXYZ\n| a | b |\n| --- | --- |\n| 1 | 2 |\n", "{out}");
}

#[test]
fn a_note_starting_with_a_table_gets_a_line_typed_before_it() {
    // What was typed there went into the first cell (`| Title a |`), Return as a space.
    let table = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
    let typed = |text: &str| {
        let (vault, out) = probe_notes(&[("a.md", table)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", text), ("KNOTES_PROBE_TYPE_CHARS", "1"), ("KNOTES_PROBE_RANGE", "0,0"), ("KNOTES_PROBE_ACTION", "save")]);
        (read(vault.path(), "a.md"), out)
    };
    let (text, out) = typed("Назва");
    assert_eq!(text, format!("Назва\n{table}"), "{out}");
    let (text, out) = typed("\n");
    assert_eq!(text, format!("\n{table}"), "{out}");
}

#[test]
fn the_line_break_above_a_table_is_not_deleted_into_its_header() {
    // Deleted, the line above joined the header (`Intro| a | b |`): a table no more.
    let table = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
    let deleted = |note: &str, range: &str| {
        let (vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", ""), ("KNOTES_PROBE_RANGE", range), ("KNOTES_PROBE_ACTION", "save")]);
        (read(vault.path(), "a.md"), out)
    };
    let (text, out) = deleted(&format!("Intro\n{table}"), "5,1");
    assert_eq!(text, format!("Intro\n{table}"), "{out}");
    let (text, out) = deleted(&format!("Intro\n\n{table}"), "6,1");
    assert_eq!(text, format!("Intro\n{table}"), "an empty line above goes: {out}");
}

#[test]
fn empty_brackets_typed_in_a_code_block_stay_as_typed() {
    let (vault, out) = probe_notes(&[("a.md", "```md\n\n```\nend")], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_TYPE", "- [] x"), ("KNOTES_PROBE_TYPE_CHARS", "1"), ("KNOTES_PROBE_RANGE", "6,0"), ("KNOTES_PROBE_ACTION", "save")]);
    assert_eq!(read(vault.path(), "a.md"), "```md\n- [] x\n```\nend", "{out}");
}

/// A vault for keeping things from the AI (ADR 0036): a diary folder, a note, a PDF.
fn ai_vault() -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in [("Щоденник/день.md", "Мій день."), ("Щоденник/Відкрите/план.md", "План."), ("нотатка.md", "Текст #no-ai тут."), ("Docs/a.pdf", "%PDF-1.4")] {
        let path = vault.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    vault
}

/// The rows a probe's report line lists, sorted.
fn listed<'a>(out: &'a str, line: &str) -> Vec<&'a str> {
    let rows = out.lines().find_map(|l| l.strip_prefix(line)).unwrap_or_else(|| panic!("no {line}: {out}"));
    let mut rows: Vec<&str> = rows.split(" | ").filter(|r| !r.is_empty()).collect();
    rows.sort();
    rows
}

#[test]
fn a_note_kept_from_the_ai_from_its_menu_is_tagged_marked_and_shown_again() {
    let vault = ai_vault();
    let day = vault.path().join("Щоденник/день.md");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Щоденник/день.md"), ("KNOTES_PROBE_ACTION", "list-ai")]);
    assert!(out.contains("list-menu-items: Відкрити в новій вкладці | Перейменувати | Перемістити в… | Додати зірочку | Сховати від ШІ | — | Видалити"), "{out}");
    assert_eq!(std::fs::read_to_string(&day).unwrap(), "---\ntags: [no-ai]\n---\nМій день.", "the open note tagged in the editor, and saved");
    let out = probe_with(&[vault.path()], None);
    assert_eq!(listed(&out, "list-ai-marks: "), ["день", "нотатка"], "a tag in the text counts too");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Щоденник/день.md"), ("KNOTES_PROBE_ACTION", "list-ai")]);
    assert!(out.contains("| Показати ШІ |"), "{out}");
    assert_eq!(std::fs::read_to_string(&day).unwrap(), "Мій день.", "the tag gone with the frontmatter it made");
}

#[test]
fn a_note_not_open_is_kept_from_the_ai_on_the_disk_and_its_tag_in_the_text_goes_when_shown() {
    let vault = ai_vault();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Щоденник/день.md"), ("KNOTES_PROBE_ACTION", "ai-path=нотатка.md")]);
    assert_eq!(std::fs::read_to_string(vault.path().join("нотатка.md")).unwrap(), "Текст тут.", "shown: its #no-ai taken out of the text");
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Щоденник/день.md"), ("KNOTES_PROBE_ACTION", "ai-path=нотатка.md")]);
    assert_eq!(std::fs::read_to_string(vault.path().join("нотатка.md")).unwrap(), "---\ntags: [no-ai]\n---\nТекст тут.");
    assert_eq!(std::fs::read_to_string(vault.path().join("Щоденник/день.md")).unwrap(), "Мій день.", "the note open untouched");
}

#[test]
fn a_folder_kept_from_the_ai_from_its_menu_marks_itself_and_the_notes_in_it_subfolders_too() {
    let vault = ai_vault();
    let rules = |folder: &str| std::fs::read_to_string(vault.path().join(folder).join(".knotes.json")).ok().map(|t| serde_json::from_str::<serde_json::Value>(&t).unwrap());
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-ai=Щоденник")]);
    assert!(out.contains("sidebar-menu-items: Перейменувати | Нова підтека | Додати зірочку | Сховати від ШІ | Видалити | Простір"), "{out}");
    assert_eq!(rules("Щоденник"), Some(serde_json::json!({"ai": false})));
    let out = probe_with(&[vault.path()], None);
    assert_eq!(listed(&out, "sidebar-ai-marks: "), ["Щоденник"], "{out}");
    assert_eq!(listed(&out, "list-ai-marks: "), ["день", "нотатка", "план"], "the subfolder's note too: {out}");
    // A subfolder shown again says so itself; a note in the folder shown again, in its rules.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-ai=Щоденник/Відкрите")]);
    assert!(out.contains("| Показати ШІ |"), "{out}");
    assert_eq!(rules("Щоденник/Відкрите"), Some(serde_json::json!({"ai": true})));
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "ai-path=Щоденник/день.md")]);
    assert_eq!(rules("Щоденник"), Some(serde_json::json!({"ai": false, "files": {"день.md": {"ai": true}}})));
    let out = probe_with(&[vault.path()], None);
    assert_eq!(listed(&out, "list-ai-marks: "), ["нотатка"], "{out}");
    // The folder shown again: its key goes, its notes' too would be said where they differ.
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-ai=Щоденник")]);
    assert_eq!(rules("Щоденник"), Some(serde_json::json!({"files": {"день.md": {"ai": true}}})));
}

#[test]
fn a_pdf_is_kept_from_the_ai_in_its_folder_s_rules_and_shown_again() {
    let vault = ai_vault();
    let rules = || std::fs::read_to_string(vault.path().join("Docs/.knotes.json")).ok().map(|t| serde_json::from_str::<serde_json::Value>(&t).unwrap());
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "ai-path=Docs/a.pdf")]);
    assert_eq!(rules(), Some(serde_json::json!({"files": {"a.pdf": {"ai": false}}})));
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "ai-path=Docs/a.pdf")]);
    assert_eq!(rules(), Some(serde_json::json!({})), "its entry gone, and the files with it");
}

#[test]
fn a_folder_whose_rules_do_not_parse_is_not_written_over_when_kept_from_the_ai() {
    let vault = ai_vault();
    let broken = r#"{"space": "work",}"#;
    std::fs::write(vault.path().join("Щоденник/.knotes.json"), broken).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "folder-ai=Щоденник")]);
    assert!(out.contains("failed: Не вдалося сховати «Щоденник» від ШІ"), "told: {out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Щоденник/.knotes.json")).unwrap(), broken);
}

#[test]
fn a_file_kept_from_the_ai_stays_kept_when_renamed_or_moved() {
    let vault = ai_vault();
    let rules = |folder: &str| std::fs::read_to_string(vault.path().join(folder).join(".knotes.json")).ok().map(|t| serde_json::from_str::<serde_json::Value>(&t).unwrap());
    std::fs::write(vault.path().join("Docs/.knotes.json"), r#"{"space": "work", "files": {"a.pdf": {"ai": false}}}"#).unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "list-rename-path=Docs/a.pdf=Договір")]);
    assert!(vault.path().join("Docs/Договір.pdf").exists());
    assert_eq!(rules("Docs"), Some(serde_json::json!({"space": "work", "files": {"Договір.pdf": {"ai": false}}})), "the new name kept, the old one gone");
    std::fs::create_dir(vault.path().join("Архів")).unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "drop-path=Docs/Договір.pdf=Архів")]);
    assert!(vault.path().join("Архів/Договір.pdf").exists());
    assert_eq!(rules("Docs"), Some(serde_json::json!({"space": "work"})));
    assert_eq!(rules("Архів"), Some(serde_json::json!({"files": {"Договір.pdf": {"ai": false}}})), "its setting went with it");
    // A note shown in a folder kept takes that with it as well.
    std::fs::write(vault.path().join("Щоденник/.knotes.json"), r#"{"ai": false, "files": {"день.md": {"ai": true}}}"#).unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "list-rename-path=Щоденник/день.md=Вівторок")]);
    assert_eq!(rules("Щоденник"), Some(serde_json::json!({"ai": false, "files": {"Вівторок.md": {"ai": true}}})));
}

#[test]
fn a_file_kept_from_the_ai_is_not_moved_where_that_cannot_be_written() {
    let vault = ai_vault();
    std::fs::write(vault.path().join("Docs/.knotes.json"), r#"{"files": {"a.pdf": {"ai": false}}}"#).unwrap();
    std::fs::create_dir(vault.path().join("Архів")).unwrap();
    let broken = r#"{"space": "work",}"#;
    std::fs::write(vault.path().join("Архів/.knotes.json"), broken).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "drop-path=Docs/a.pdf=Архів")]);
    assert!(out.contains("failed: «a» не переміщено, щоб його не побачив ШІ"), "told: {out}");
    assert!(vault.path().join("Docs/a.pdf").exists() && !vault.path().join("Архів/a.pdf").exists(), "not moved");
    assert_eq!(std::fs::read_to_string(vault.path().join("Архів/.knotes.json")).unwrap(), broken);
    // A file with no setting of its own moves there as ever.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "drop-path=нотатка.md=Архів")]);
    assert!(vault.path().join("Архів/нотатка.md").exists(), "{out}");
}

#[test]
fn a_file_kept_from_the_ai_restored_under_another_name_stays_kept() {
    let vault = ai_vault();
    let rules = || std::fs::read_to_string(vault.path().join("Docs/.knotes.json")).ok().map(|t| serde_json::from_str::<serde_json::Value>(&t).unwrap());
    std::fs::write(vault.path().join("Docs/.knotes.json"), r#"{"files": {"a.pdf": {"ai": false}}}"#).unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_ACTION", "delete-path=Docs/a.pdf")]);
    assert!(!vault.path().join("Docs/a.pdf").exists());
    assert_eq!(rules(), Some(serde_json::json!({"files": {"a.pdf": {"ai": false}}})), "kept while deleted: restored to its place, it is kept still");
    std::fs::write(vault.path().join("Docs/a.pdf"), "%PDF-1.4 another").unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "deleted"), ("KNOTES_PROBE_ACTION", "list-restore")]);
    assert!(vault.path().join("Docs/a 2.pdf").exists(), "back under another name");
    assert_eq!(rules(), Some(serde_json::json!({"files": {"a.pdf": {"ai": false}, "a 2.pdf": {"ai": false}}})));
}

#[test]
fn a_pdf_written_again_outside_while_open_is_shown_anew_where_it_was() {
    let shorter = tempfile::tempdir().unwrap();
    let new = shorter.path().join("new.pdf");
    std::fs::write(&new, pdf_of(3)).unwrap();
    let rewrite = format!("long.pdf={}", new.display());
    let env = [("KNOTES_PROBE_OPEN", "long.pdf"), ("KNOTES_PROBE_PDF_GO", "2"), ("KNOTES_PROBE_REWRITE_IMAGE", &rewrite), ("KNOTES_PROBE_DELAY_MS", "2500")];
    let (_vault, out) = probe_images(&[], &[("long.pdf", &pdf_of(30))], &env);
    assert!(out.contains("pdf-view: long.pdf 2/3"), "its 3 pages now, at the page it was at: {out}");
    // Fewer pages than the one it was at: the last.
    std::fs::write(&new, pdf_of(1)).unwrap();
    let (_vault, out) = probe_images(&[], &[("long.pdf", &pdf_of(30))], &env);
    assert!(out.contains("pdf-view: long.pdf 1/1"), "{out}");
}

#[test]
fn a_pdf_deleted_outside_while_open_closes() {
    let env = [("KNOTES_PROBE_OPEN", "long.pdf"), ("KNOTES_PROBE_DELETE", "1"), ("KNOTES_PROBE_DELAY_MS", "2500")];
    let (_vault, out) = probe_images(&[("n.md", "n")], &[("long.pdf", &pdf_of(30))], &env);
    assert!(out.contains("pdf-view: hidden") && out.contains("note-column: empty"), "{out}");
}

#[test]
fn the_page_shown_follows_the_scroller_after_a_pdf_opened_or_jumped() {
    let notes = [("a.md", "See [[long.pdf]] here\n\nend")];
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    for jump in [None, Some("15")] {
        let mut env = open.to_vec();
        env.extend(jump.map(|page| ("KNOTES_PROBE_PDF_GO", page)));
        env.push(("KNOTES_PROBE_PDF_SCROLLER", "3"));
        let (_vault, out) = probe_images(&notes, &[("long.pdf", &pdf_of(30))], &env);
        assert!(out.contains("pdf-view: long.pdf 3/30") && out.contains("pdf-page-label: сторінка 3 з 30"), "{jump:?}: {out}");
        let kept = pages_kept(&out);
        assert!(kept.contains(&3) && kept.iter().all(|p| (1..=5).contains(p)), "{jump:?}: around page 3 only: {kept:?}");
    }
}

#[test]
fn a_pdf_that_does_not_open_says_why() {
    let notes = [("a.md", "See [[broken.pdf]] here\n\nend")];
    let env = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", "end"), ("KNOTES_PROBE_CLICK", "8")];
    let (_vault, out) = probe_images(&notes, &[("broken.pdf", b"not a PDF at all")], &env);
    assert!(out.contains("failed: Не вдалося відкрити «broken»: Файл пошкоджений або це не PDF."), "{out}");
    assert!(out.contains("note-column: empty"), "{out}");
}

#[test]
fn the_settings_say_why_search_by_meaning_fails_instead_of_the_vectors_being_up_to_date() {
    let (_vault, out) = probe_notes(&SEARCH_NOTES, &[("KNOTES_PROBE_SETTINGS", "1"), ("KNOTES_PROBE_MEANING_FAILING", "no model at /x/model.gguf")]);
    assert!(out.contains("settings-progress: Пошук за змістом не працює: no model at /x/model.gguf\n"), "{out}");
    let (_vault, out) = probe_notes(&SEARCH_NOTES, &[("KNOTES_PROBE_SETTINGS", "1")]);
    assert!(out.contains("settings-progress: Вектори актуальні\n"), "{out}");
}

#[test]
fn the_path_bar_shows_where_the_note_is_and_opens_its_folders() {
    let notes = [("Дім/Техніка/Бойлер.md", "Бойлер"), ("top.md", "top")];
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Дім/Техніка/Бойлер.md")]);
    assert!(out.contains("path-bar: 📕 › Дім › Техніка › Бойлер\n"), "the vault (its symbol alone), its folders, the note: {out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Дім/Техніка/Бойлер.md"), ("KNOTES_PROBE_ACTION", "path-bar=2")]);
    assert!(out.contains("title: Техніка\n") && out.contains("sidebar-selected: Техніка\n"), "a folder clicked shows it, its row opened to: {out}");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "Дім/Техніка/Бойлер.md"), ("KNOTES_PROBE_ACTION", "path-bar=0")]);
    assert!(out.contains("title: Усі нотатки\n"), "the vault: all notes: {out}");
    let (_vault, out) = probe_notes(&notes, &[]);
    assert!(out.contains("path-bar: hidden\n"), "no document open: {out}");
}

/// The vault's notes, by their path, with what they hold.
fn vault_notes(vault: &std::path::Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut folders = vec![vault.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                found.push((path.strip_prefix(vault).unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_new_note_starts_with_its_name_to_type_and_keeps_the_name_typed() {
    let (_vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "new")]);
    assert!(out.contains("note-title-focus: editing\n") && out.contains("note-title: \n"), "its name to type, the field empty («Без назви» grey in it): {out}");
    // Named there, it is not named again after its first line.
    let (vault, out) = probe_notes(&[("a.md", "a")], &[("KNOTES_PROBE_ACTION", "new"), ("KNOTES_PROBE_TITLE_NAME", "Мій план"), ("KNOTES_PROBE_TYPE", "Перший рядок\nдалі")]);
    let notes = vault_notes(vault.path());
    assert!(notes.iter().any(|(path, text)| path.ends_with("Мій план.md") && text == "Перший рядок\nдалі"), "{notes:?} {out}");
    assert!(!notes.iter().any(|(path, _)| path.contains("Перший рядок")), "{notes:?}");
}

#[test]
fn the_find_bar_s_aa_keeps_the_case_when_switched_off() {
    let note = "Пункт один.\n\nДругий пункт і ще ПУНКТ.\n\nкінець";
    let open = [("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_FIND", "пункт")];
    let (_vault, out) = probe_notes(&[("a.md", note)], &open);
    assert!(out.contains("found-in-note: 3"), "any case, as it starts: {out}");
    let (_vault, out) = probe_notes(&[("a.md", note)], &[open[0], open[1], ("KNOTES_PROBE_FIND_CASE", "1")]);
    assert!(out.contains("find: 1 з 1") && out.contains("found-in-note: 1"), "only «пункт» as typed: {out}");
}

#[test]
fn the_list_search_s_aa_keeps_the_case_when_switched_off() {
    let notes = [("Radar.md", "Про РАДАР і ще щось."), ("radar plan.md", "Тут радар малими."), ("other.md", "нічого")];
    let titles = |out: &str| -> Vec<String> {
        let list = out.lines().find_map(|l| l.strip_prefix("list: ")).unwrap_or_else(|| panic!("{out}"));
        let mut titles: Vec<String> = list.split(" | ").filter(|r| !r.starts_with('[')).map(|r| r.split(" ~ ").next().unwrap_or_default().to_owned()).filter(|t| !t.is_empty()).collect();
        titles.sort();
        titles
    };
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_SEARCH", "radar")]);
    assert_eq!(titles(&out), ["Radar", "radar plan"], "any case, as it starts");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_SEARCH", "radar"), ("KNOTES_PROBE_SEARCH_CASE", "1")]);
    assert_eq!(titles(&out), ["radar plan"], "the name as typed");
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_SEARCH", "content: радар")]);
    assert_eq!(titles(&out), ["Radar", "radar plan"]);
    let (_vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_SEARCH", "content: радар"), ("KNOTES_PROBE_SEARCH_CASE", "1")]);
    assert_eq!(titles(&out), ["radar plan"], "the text as typed");
}

#[test]
fn the_sidebar_s_cards_mode_lists_the_decks_with_their_cards_to_review_and_a_deck_s_notes() {
    let notes = [
        ("Мови/Англійська.md", "Слова.\n\n#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n"),
        ("Мови/Німецька.md", "#@мови/німецька Hund::пес ^c-3\n"),
        ("Історія.md", "#@історія Коли?\n?\nТоді. ^c-4\n"),
        ("Без карток.md", "звичайна нотатка"),
    ];
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_SIDEBAR", "cards")]);
    assert!(out.contains("sidebar-mode: cards\n"), "{out}");
    assert!(out.contains("sidebar: Усі колоди (4) | історія (1) | мови (3)\n"), "the decks, each with its cards and its sub-decks' to review: {out}");
    let (_, out) = (0, probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")]));
    assert!(out.contains("title: мови\n"), "{out}");
    let mut listed = list_titles(&out);
    listed.sort();
    assert_eq!(listed, ["Англійська", "Німецька"], "the notes its cards are written in: {out}");
    // Reviewed a moment ago and graded Good: due in three days, no longer waiting.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::create_dir_all(vault.path().join(".knotes/reviews")).unwrap();
    std::fs::write(vault.path().join(".knotes/reviews/mac.tsv"), format!("{now}\tc-1\t3\n{now}\tc-4\t3\n")).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards")]);
    assert!(out.contains("sidebar: Усі колоди (2) | історія (0) | мови (2)\n"), "{out}");
}

/// A vault with cards in two decks.
const CARD_NOTES: [(&str, &str); 2] = [
    ("Мови.md", "Слова.\n\n#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес\n"),
    ("Історія.md", "#@історія Коли була битва?\n?\n1659 року. ^c-4\n"),
];

fn review_line_of(out: &str) -> &str {
    out.lines().find_map(|l| l.strip_prefix("review: ")).unwrap_or_else(|| panic!("{out}"))
}

#[test]
fn a_deck_s_cards_show_as_a_table_and_a_card_s_text_is_changed_there() {
    let line = |out: &str, name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("{name}: {out}")).to_owned();
    // A deck's menu → «Таблиця карток»: its cards and those under it, as their notes have
    // them, the first chosen with its text below.
    let (vault, out) = probe_notes(&CARD_NOTES, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_DECK_TABLE", "мови")]);
    assert_eq!(
        line(&out, "deck-table: "),
        "Картки: мови — 2 (2 cards, as written): [cat · мови/англійська · нова] | dog · мови/англійська · нова ~ #@мови/англійська cat::кіт ^c-1",
        "{out}"
    );
    // All the decks together, sorted by their decks; the second card's text typed over and
    // written: its note has it, the rest of the note as it was, and the row says the new
    // question. The card after it in the note is still found (it has no id: by its place).
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_DECK_TABLE", ""), ("KNOTES_PROBE_DECK_TABLE_SORT", "deck"), ("KNOTES_PROBE_DECK_TABLE_EDIT", "1=#@мови/англійська кішка::cat ^c-1")]);
    assert!(line(&out, "deck-table: ").starts_with("Картки: Усі колоди — 3 (3 cards, Deck up): [Коли була битва? · історія · нова] | cat · мови/англійська · нова | dog · "), "{out}");
    assert_eq!(line(&out, "deck-table-edit: "), "written", "{out}");
    assert!(line(&out, "deck-table-after: ").contains("| [кішка · мови/англійська · нова] | dog · мови/англійська · нова ~ #@мови/англійська кішка::cat ^c-1"), "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Мови.md")).unwrap(), "Слова.\n\n#@мови/англійська кішка::cat ^c-1\n\n#@мови/англійська dog::пес\n");
    // A reviewed card says when it comes and how it stands; sorted by its reviews, it is first
    // and the new ones after it.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови"), ("KNOTES_PROBE_REVIEW", "start,show,4")]);
    assert!(review_line_of(&out).starts_with("question мови 1/1 | dog"), "{out}");
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_DECK_TABLE", "мови"), ("KNOTES_PROBE_DECK_TABLE_SORT", "reviews")]);
    let table = line(&out, "deck-table: ");
    assert!(table.contains("Reviews up): [кішка · мови/англійська · через ") && table.contains(" · 1 · 0] | dog · мови/англійська · нова"), "{table}");
    // The search over the table: the cards whose own text holds the words (the other card of
    // the same note does not), or of a deck named so; the title says how many of how many.
    let find = |query: &str| line(&probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_DECK_TABLE", ""), ("KNOTES_PROBE_DECK_TABLE_FIND", query)]), "deck-table: ");
    assert!(find("пес").starts_with("Картки: Усі колоди — 1 з 3 (1 cards, as written): [dog · мови/англійська · нова] ~ #@мови/англійська dog::пес"), "{}", find("пес"));
    assert!(find("ІСТОР").starts_with("Картки: Усі колоди — 1 з 3 (1 cards, as written): [Коли була битва? · історія · "), "a deck by its name, any case: {}", find("ІСТОР"));
    assert!(find("битва 1659").contains("— 1 з 3 "), "every word, anywhere in the card: {}", find("битва 1659"));
    assert_eq!(find("zzz"), "Картки: Усі колоди — 0 з 3 (0 cards, as written):  ~ ");
    // A deck with no cards under that name: an empty table, nothing to change.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_DECK_TABLE", "немає"), ("KNOTES_PROBE_DECK_TABLE_EDIT", "0=x")]);
    assert_eq!(line(&out, "deck-table: "), "Картки: немає — 0 (0 cards, as written):  ~ ", "{out}");
    assert_eq!(std::fs::read_to_string(vault.path().join("Мови.md")).unwrap().matches("кішка").count(), 1);
}

#[test]
fn rows_chosen_together_are_paused_put_in_a_deck_and_deleted_together() {
    let notes = [("A.md", "#@en cat::кіт ^c-1\n\n#@en dog::пес ^c-2\n\n#@en cow::корова ^c-3\n"), ("B.md", "#@de Hund::пес ^c-4\n")];
    let line = |out: &str, name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("{name}: {out}")).to_owned();
    let table = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_DECK_TABLE", "")];
    // A row clicked, another with ⌘: both chosen, the last one's card shown.
    let (_vault, out) = probe_notes(&notes, &[table[0], table[1], ("KNOTES_PROBE_DECK_TABLE_ACT", "0+2=choose")]);
    assert!(line(&out, "deck-table-acted: ").contains("(cat · en · нова) | dog · en · нова | [cow · en · нова] | Hund · de · нова ~ #@en cow::корова ^c-3"), "{out}");
    // cat and cow paused, dog and Hund (of two notes) put in a deck, cat and dog deleted.
    let (vault, out) = probe_notes(&notes, &[table[0], table[1], ("KNOTES_PROBE_DECK_TABLE_ACT", "0+2=pause;1+3=deck;0+1=delete"), ("KNOTES_PROBE_CARD", "zoo|")]);
    let acted = line(&out, "deck-table-acted: ");
    assert!(acted.contains("(2 cards, as written): [cow · en · призупинено] | Hund · zoo · нова ~ #@en cow::корова ^c-3"), "{acted}");
    let read = |name: &str| std::fs::read_to_string(vault.path().join(name)).unwrap();
    assert_eq!((read("A.md").as_str(), read("B.md").as_str()), ("#@en cow::корова ^c-3\n", "#@zoo Hund::пес ^c-4\n"), "{out}");
    let logs: Vec<_> = std::fs::read_dir(vault.path().join(".knotes/reviews")).unwrap().flatten().collect();
    let log = std::fs::read_to_string(logs[0].path()).unwrap();
    assert!(log.lines().count() == 2 && log.contains("\tc-1\tP\n") && log.contains("\tc-3\tP\n"), "{log}");
}

#[test]
fn a_card_is_paused_put_in_another_deck_and_deleted_from_its_row_s_menu() {
    let notes = [("Мови.md", "Слова.\n\n#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес\n\n#@мови/німецька Hund::пес ^c-3\n")];
    let line = |out: &str, name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("{name}: {out}")).to_owned();
    let table = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_DECK_TABLE", "")];
    // dog (typed, no id yet) paused, cat put in another deck, Hund deleted.
    let (vault, out) = probe_notes(&notes, &[table[0], table[1], ("KNOTES_PROBE_DECK_TABLE_ACT", "1=pause;0=deck;2=delete"), ("KNOTES_PROBE_CARD", "тварини|")]);
    let acted = line(&out, "deck-table-acted: ");
    assert!(acted.starts_with("Картки: Усі колоди — 2 (2 cards, as written): cat · тварини · нова | [dog · мови/англійська · призупинено] ~ #@мови/англійська dog::пес ^c-"), "{acted}");
    // The note: cat's mark says its new deck, dog has the id it was paused by, Hund is gone,
    // the rest as it was.
    let note = std::fs::read_to_string(vault.path().join("Мови.md")).unwrap();
    let id = note.split("dog::пес ^").nth(1).and_then(|rest| rest.split('\n').next()).unwrap_or_else(|| panic!("{note}")).to_owned();
    assert_eq!(note, format!("Слова.\n\n#@тварини cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^{id}\n"), "no gap where Hund was: {out}");
    // The log says the pause, by the card's id.
    let logs: Vec<_> = std::fs::read_dir(vault.path().join(".knotes/reviews")).unwrap().flatten().collect();
    let log = std::fs::read_to_string(logs[0].path()).unwrap();
    assert!(log.ends_with(&format!("\t{id}\tP\n")) && log.lines().count() == 1, "{log}");
    // The next launch: the card paused does not wait (its deck asks nothing), and is not asked
    // with all the decks; the table still has it, paused.
    let out = probe_env(&[vault.path()], None, &table);
    assert!(out.contains("sidebar: Усі колоди (1) | мови (0) | тварини (1)\n"), "{out}");
    assert!(line(&out, "deck-table: ").contains("| dog · мови/англійська · призупинено ~ "), "{out}");
    // In its note the paused card shows muted, all of it; the other as a card does.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_OPEN", "Мови.md"), ("KNOTES_PROBE_CURSOR", "0")]);
    let runs = editor_runs(&out);
    assert!(runs.contains(&format!("[card-paused]#@мови/англійська dog::пес ^{id}|")), "{runs}");
    assert!(runs.contains("[card-deck]#@тварини|") && runs.contains("[card-question]cat|"), "{runs}");
    let out = probe_env(&[vault.path()], None, &[table[0], ("KNOTES_PROBE_SELECT", "d:"), ("KNOTES_PROBE_REVIEW", "start,show,4")]);
    assert_eq!(review_line_of(&out), "done Усі колоди 1", "cat alone was asked: {out}");
    // Resumed from the same menu: it waits again.
    let out = probe_env(&[vault.path()], None, &[table[0], table[1], ("KNOTES_PROBE_DECK_TABLE_ACT", "1=pause")]);
    assert!(line(&out, "deck-table-acted: ").contains("[dog · мови/англійська · нова]"), "{out}");
    let out = probe_env(&[vault.path()], None, &table);
    assert!(out.contains("| мови (1) | "), "{out}");
}

#[test]
fn a_deck_s_reviews_a_day_limit_the_due_cards_it_asks() {
    // cat was graded Easy a month ago on another Mac: due for two weeks now. dog is new.
    let (vault, _) = probe_notes(&CARD_NOTES, &[]);
    std::fs::create_dir_all(vault.path().join(".knotes/reviews")).unwrap();
    std::fs::write(vault.path().join(".knotes/reviews/other.tsv"), format!("{}\tc-1\t4\n", run_now() - 30 * 86_400)).unwrap();
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    let out = probe_env(&[vault.path()], None, &open);
    assert!(out.contains("sidebar: Усі колоди (3) | історія (1) | мови (2)\n"), "the due one and the new one: {out}");
    assert_eq!(review_line_of(&out), "waiting мови 2", "{out}");
    // The deck's settings: no reviews a day (the fourth field). The due card waits for
    // another day; the new one is still asked. The deck says so at once, and the next time.
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_DECK_SETTINGS", "|||0")]);
    assert!(out.contains("sidebar: Усі колоди (2) | історія (1) | мови (1)\n"), "counted again at once: {out}");
    let settings = std::fs::read_to_string(vault.path().join(".knotes/decks.json")).unwrap();
    assert!(settings.contains("\"reviews_a_day\": 0"), "{settings}");
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start")]);
    assert_eq!(review_line_of(&out), "question мови 0/1 | dog (list hidden)", "the new card alone: {out}");
    // One a day: the due card is asked again; an empty field takes the limit away.
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_DECK_SETTINGS", "|||1"), ("KNOTES_PROBE_REVIEW", "start")]);
    assert_eq!(review_line_of(&out), "question мови 0/2 | cat (list hidden)", "{out}");
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_DECK_SETTINGS", "|||")]);
    assert!(out.contains("| мови (2)\n") && !std::fs::read_to_string(vault.path().join(".knotes/decks.json")).unwrap().contains("reviews_a_day"), "{out}");
}

#[test]
fn the_decks_first_row_reviews_all_of_them_together() {
    // Above the decks, «Усі колоди», with what they ask between them; chosen, its review takes
    // every deck's cards, in a tab of that name.
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:")];
    let (vault, out) = probe_notes(&CARD_NOTES, &open);
    let sidebar = out.lines().find_map(|l| l.strip_prefix("sidebar: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    assert!(sidebar.starts_with("Усі колоди (3) | "), "{sidebar}");
    assert_eq!(review_line_of(&out), "waiting Усі колоди 3", "{out}");
    assert!(out.contains("title: Усі колоди\n") && out.contains("tabs: Усі колоди\n"), "{out}");
    // The cards of every deck are asked, one after another.
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,4,show,4,show,4")]);
    assert_eq!(review_line_of(&out), "done Усі колоди 3", "{out}");
    // Reviewed, they wait no more, in their decks or together.
    let out = probe_env(&[vault.path()], None, &open);
    assert_eq!(review_line_of(&out), "waiting Усі колоди 0", "{out}");
}

#[test]
fn a_deck_chosen_shows_its_cards_to_review_and_the_review_asks_then_answers() {
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    let (_vault, out) = probe_notes(&CARD_NOTES, &open);
    assert_eq!(review_line_of(&out), "waiting мови 2", "{out}");
    let (_vault, out) = probe_notes(&CARD_NOTES, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start")]);
    assert_eq!(review_line_of(&out), "question мови 0/2 | cat (list hidden)", "the first card's question, its answer not shown: {out}");

    let (_vault, out) = probe_notes(&CARD_NOTES, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show")]);
    assert_eq!(review_line_of(&out), "answer мови 0/2 | cat | кіт | Знову · 1 хв, Важко · 6 хв, Добре · 10 хв, Легко · 16 д (list hidden)", "a new card is learned in steps; the list's column hidden while a card shows: {out}");
    let (_vault, out) = probe_notes(&CARD_NOTES, &[open[0], ("KNOTES_PROBE_SELECT", "d:історія"), ("KNOTES_PROBE_REVIEW", "start,show")]);
    assert_eq!(review_line_of(&out), "answer історія 0/1 | Коли була битва? | 1659 року. | Знову · 1 хв, Важко · 6 хв, Добре · 10 хв, Легко · 16 д (list hidden)", "{out}");
    // Another list chosen: the review goes.
    let (_vault, out) = probe_notes(&CARD_NOTES, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start"), ("KNOTES_PROBE_OPEN", "Мови.md")]);
    assert_eq!(review_line_of(&out), "hidden", "a note opened takes the column: {out}");
}

#[test]
fn grading_a_card_logs_its_review_gives_it_an_id_and_goes_on() {
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    // cat graded Good, dog (no id yet) graded Again: both are still learned (their steps), and
    // come back at the review's end, cat first.
    let (vault, out) = probe_notes(&CARD_NOTES, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,3,show,1")]);
    assert_eq!(review_line_of(&out), "question мови 2/2 | cat (list hidden)", "still learned, they are asked again: {out}");
    let logs: Vec<_> = std::fs::read_dir(vault.path().join(".knotes/reviews")).unwrap().flatten().collect();
    assert_eq!(logs.len(), 1, "this Mac's log");
    let log = std::fs::read_to_string(logs[0].path()).unwrap();
    let lines: Vec<Vec<&str>> = log.lines().map(|l| l.split('\t').collect()).collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert_eq!((lines[0][1], lines[0][2]), ("c-1", "3"));
    let note = std::fs::read_to_string(vault.path().join("Мови.md")).unwrap();
    let given = lines[1][1];
    assert!(given.starts_with("c-") && note.contains(&format!("#@мови/англійська dog::пес ^{given}\n")) && lines[1][2] == "1", "its id written at its end: {note} {log}");
    // Next time: cat is due in ten minutes, dog in one; nothing waits now.
    let out = probe_env(&[vault.path()], None, &open);
    assert_eq!(review_line_of(&out), "waiting мови 0", "{out}");
    assert!(out.contains("sidebar: Усі колоди (1) | історія (1) | мови (0)\n"), "{out}");
    // Ahead of the schedule (owner's button): dog, due in a minute, then cat, in ten;
    // graded, the review is logged as any other.
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "ahead")]);
    assert_eq!(review_line_of(&out), "question мови 0/2 | dog (list hidden)", "{out}");
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "ahead,show,3,show")]);
    assert!(review_line_of(&out).starts_with("answer мови 1/2 | cat | кіт | Знову · 1 хв, "), "{out}");
    // A launch of the test is a Mac of its own, with its own log.
    let logged: usize = std::fs::read_dir(vault.path().join(".knotes/reviews")).unwrap().flatten().map(|log| std::fs::read_to_string(log.path()).unwrap().lines().count()).sum();
    assert_eq!(logged, 3, "{out}");
    // A deck with nothing reviewed has nothing ahead: the button asks nothing.
    let (_vault, out) = probe_notes(&CARD_NOTES, &[open[0], ("KNOTES_PROBE_SELECT", "d:історія"), ("KNOTES_PROBE_REVIEW", "ahead")]);
    assert_eq!(review_line_of(&out), "waiting історія 1", "{out}");
}

#[test]
fn a_deck_s_settings_limit_its_new_cards_and_move_its_intervals() {
    let notes = [("Мови.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n\n#@мови/німецька Hund::пес ^c-3\n"), ("Історія.md", "#@історія Коли?::Тоді ^c-4\n")];
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    // Set from the deck's menu: recalled at 85%, two new cards a day, no learning steps.
    // Written to the vault.
    let (vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_DECK_SETTINGS", "85|2|0")]);
    let file = vault.path().join(".knotes/decks.json");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\n  \"мови\": {\n    \"new_a_day\": 2,\n    \"retention\": 0.85,\n    \"steps\": []\n  }\n}\n", "{out}");
    assert!(out.contains("sidebar: Усі колоди (3) | історія (1) | мови (2)\n"), "counted again at once: {out}");
    assert_eq!(review_line_of(&out), "waiting мови 2", "{out}");
    // The next launch: the same, and the intervals longer (a card forgotten more often is seen less).
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show")]);
    assert_eq!(review_line_of(&out), "answer мови 0/2 | cat | кіт | Знову · 1 д, Важко · 2 д, Добре · 5 д, Легко · 26 д (list hidden)", "{out}");
    // One of the day's two taken (cat, reviewed a moment ago): one more new card today.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::create_dir_all(vault.path().join(".knotes/reviews")).unwrap();
    std::fs::write(vault.path().join(".knotes/reviews/mac.tsv"), format!("{now}\tc-1\t3\n")).unwrap();
    let out = probe_env(&[vault.path()], None, &open);
    assert!(out.contains("sidebar: Усі колоди (2) | історія (1) | мови (1)\n"), "{out}");
    assert_eq!(review_line_of(&out), "waiting мови 1", "{out}");
    // Its steps set: a new card comes back in minutes before its days.
    let out = probe_env(&[vault.path()], None, &[open[0], ("KNOTES_PROBE_SELECT", "d:мови/німецька"), ("KNOTES_PROBE_DECK_SETTINGS", "||2, 20"), ("KNOTES_PROBE_REVIEW", "start,show")]);
    assert_eq!(review_line_of(&out), "answer мови/німецька 0/1 | Hund | пес | Знову · 2 хв, Важко · 11 хв, Добре · 20 хв, Легко · 26 д (list hidden)", "{out}");
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"steps\": [\n      2,\n      20\n    ]"), "{out}");
    // A deck under it sets its own; an empty field takes the deck's above again.
    let out = probe_env(&[vault.path()], None, &[open[0], ("KNOTES_PROBE_SELECT", "d:мови/німецька"), ("KNOTES_PROBE_DECK_SETTINGS", "|0")]);
    assert!(std::fs::read_to_string(&file).unwrap().contains("\"мови/німецька\": {\n    \"new_a_day\": 0\n  }"), "{out}");
    assert_eq!(review_line_of(&out), "waiting мови/німецька 0", "{out}");
    let out = probe_env(&[vault.path()], None, &[open[0], ("KNOTES_PROBE_SELECT", "d:мови/німецька"), ("KNOTES_PROBE_DECK_SETTINGS", "|")]);
    assert!(!std::fs::read_to_string(&file).unwrap().contains("німецька"), "{out}");
    assert_eq!(review_line_of(&out), "waiting мови/німецька 1", "{out}");
}

#[test]
fn a_deck_waiting_shows_its_statistics() {
    let notes = [("Мови.md", "#@мови/англійська cat::кіт ^c-1\n\n#@мови/англійська dog::пес ^c-2\n\n#@мови/німецька Hund::пес ^c-3\n"), ("Історія.md", "#@історія Коли?::Тоді ^c-4\n")];
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    let stats = |out: &str| out.lines().find_map(|l| l.strip_prefix("deck-stats: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    let (vault, out) = probe_notes(&notes, &open);
    assert_eq!(stats(&out), "Карток: 3 — нових 3, молодих 0, зрілих 0⏎⏎Сьогодні: повторів не було⏎За 30 днів: повторів не було⏎⏎Прийде на повторення: завтра 0 · за 7 днів 0 · за 30 днів 0", "{out}");
    // cat graded Good then forgotten a moment ago (learned again), dog Easy (its days begin: sixteen).
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::create_dir_all(vault.path().join(".knotes/reviews")).unwrap();
    std::fs::write(vault.path().join(".knotes/reviews/mac.tsv"), format!("{}\tc-1\t3\n{now}\tc-1\t1\n{now}\tc-2\t4\n{now}\tc-4\t3\n", now - 1)).unwrap();
    let out = probe_env(&[vault.path()], None, &open);
    let shown = stats(&out);
    let lines: Vec<&str> = shown.split('⏎').collect();
    assert_eq!(lines[0], "Карток: 3 — нових 1, молодих 2, зрілих 0", "{out}");
    assert_eq!((lines[2], lines[3]), ("Сьогодні: 3 повтори, згадано 67 %", "За 30 днів: 3 повтори, згадано 67 %"), "the other deck's review not counted: {out}");
    assert_eq!(lines[4], format!("{}█", "▁".repeat(29)), "a bar a day, today last: {out}");
    assert_eq!(lines[6], "Прийде на повторення: завтра 0 · за 7 днів 0 · за 30 днів 1", "{out}");
    assert_eq!(lines[7].chars().filter(|c| *c == '█').count(), 1, "{out}");
    // A deck with no cards to ask still says how it stands; no deck shown, nothing said.
    let out = probe_env(&[vault.path()], None, &[open[0], ("KNOTES_PROBE_SELECT", "d:історія")]);
    assert!(stats(&out).starts_with("Карток: 1 — нових 0, молодих 1, зрілих 0⏎"), "{out}");
    assert!(!probe_env(&[vault.path()], None, &[open[0]]).contains("deck-stats:"));
}

#[test]
fn a_space_s_decks_are_those_of_its_notes_cards() {
    let vault = spaced_vault();
    for (path, text) in [
        ("Work/a.md", "#@мови cat::кіт ^c-1\n\n#@робота task::завдання ^c-2\n"),
        ("Home/c.md", "#@мови dog::пес ^c-3\n\n#@дім tea::чай ^c-4\n"),
        ("top.md", "#@мови sun::сонце ^c-5\n"),
    ] {
        std::fs::write(vault.path().join(path), text).unwrap();
    }
    // dog reviewed a moment ago: due in three days.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::create_dir_all(vault.path().join(".knotes/reviews")).unwrap();
    std::fs::write(vault.path().join(".knotes/reviews/mac.tsv"), format!("{now}\tc-3\t3\n")).unwrap();
    let cards = ("KNOTES_PROBE_SIDEBAR", "cards");
    let sidebar = |out: &str| out.lines().find_map(|l| l.strip_prefix("sidebar: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    let stats = |out: &str| out.lines().find_map(|l| l.strip_prefix("deck-stats: ")).unwrap_or_else(|| panic!("{out}")).to_owned();
    // The whole vault: every deck, with all its cards.
    let out = probe_env(&[vault.path()], None, &[cards, ("KNOTES_PROBE_SPACE", ""), ("KNOTES_PROBE_SELECT", "d:мови")]);
    assert_eq!(sidebar(&out), "Усі колоди (4) | дім (1) | мови (2) | робота (1)", "{out}");
    assert!(stats(&out).starts_with("Карток: 3 — нових 2, молодих 1, зрілих 0⏎"), "{out}");
    // Work: the decks its notes' cards are in, with those cards only.
    let out = probe_env(&[vault.path()], None, &[cards, ("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_SELECT", "d:мови")]);
    assert_eq!(sidebar(&out), "Усі колоди (2) | мови (1) | робота (1)", "{out}");
    assert_eq!(review_line_of(&out), "waiting мови 1", "{out}");
    assert_eq!(list_titles(&out), ["a"], "the deck's notes in the space: {out}");
    assert!(stats(&out).starts_with("Карток: 1 — нових 1, молодих 0, зрілих 0⏎⏎Сьогодні: повторів не було⏎"), "another space's review is not its: {out}");
    let out = probe_env(&[vault.path()], None, &[cards, ("KNOTES_PROBE_SPACE", "work"), ("KNOTES_PROBE_SELECT", "d:мови"), ("KNOTES_PROBE_REVIEW", "start")]);
    assert_eq!(review_line_of(&out), "question мови 0/1 | cat (list hidden)", "{out}");
    // Home: its own, the reviewed card not waiting but there to review ahead.
    let out = probe_env(&[vault.path()], None, &[cards, ("KNOTES_PROBE_SPACE", "home"), ("KNOTES_PROBE_SELECT", "d:мови")]);
    assert_eq!(sidebar(&out), "Усі колоди (1) | дім (1) | мови (0)", "{out}");
    assert!(stats(&out).starts_with("Карток: 1 — нових 0, молодих 1, зрілих 0⏎⏎Сьогодні: 1 повтор, згадано 100 %⏎"), "{out}");
    let out = probe_env(&[vault.path()], None, &[cards, ("KNOTES_PROBE_SPACE", "home"), ("KNOTES_PROBE_SELECT", "d:мови"), ("KNOTES_PROBE_REVIEW", "ahead")]);
    assert_eq!(review_line_of(&out), "question мови 0/1 | dog (list hidden)", "{out}");
}

#[test]
fn a_card_graded_twice_in_one_review_goes_on_from_its_first_grade() {
    // One card, already with its id: graded Good it is still learned and comes back at once.
    let notes = [("n.md", "#@мови cat::кіт ^c-1\n")];
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    // Asked again, it stands at its second step, not as a new card (the index has not heard of
    // the first grade yet: the review remembers it, as knotes-app's `review_session` tests).
    // Good again: its days begin, and the review is done. (It came back for ever: each Good
    // looked like a new card's first.)
    let (vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,3,show,3")]);
    assert_eq!(review_line_of(&out), "done мови 2", "{out}");
    // Both grades are in the log, a second apart at least, though given in one.
    let logs: Vec<_> = std::fs::read_dir(vault.path().join(".knotes/reviews")).unwrap().flatten().collect();
    let log = std::fs::read_to_string(logs[0].path()).unwrap();
    let times: Vec<i64> = log.lines().map(|l| l.split('\t').next().unwrap().parse().unwrap()).collect();
    assert!(times.len() == 2 && times[1] > times[0], "{log}");
}

#[test]
fn a_grade_reaches_the_index_while_the_app_runs() {
    // A vault reviewed before: its logs' folder is there (a folder made tells the watcher of
    // itself; a line added to a log in it must do as much).
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in CARD_NOTES {
        std::fs::write(vault.path().join(path), text).unwrap();
    }
    std::fs::create_dir_all(vault.path().join(".knotes/reviews")).unwrap();
    std::fs::write(vault.path().join(".knotes/reviews/other-mac.tsv"), "").unwrap();
    // Graded Easy, the deck's only card is no longer waiting: the sidebar says so in the same
    // launch (the watcher did not tell of the review's log, and the card stayed due until some
    // note changed).
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:історія")];
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,4"), ("KNOTES_PROBE_DELAY_MS", "2500")]);
    assert_eq!(review_line_of(&out), "done історія 1", "{out}");
    assert!(out.contains("sidebar: Усі колоди (2) | історія (0) | мови (2)\n"), "{out}");
}

#[test]
fn typed_cards_with_no_id_are_all_asked_though_each_id_written_moves_the_rest() {
    // Three cards typed in one note, none with an id: each graded is given one, which moves
    // the others on in the text. They were found by their old place, and passed over.
    let notes = [("n.md", "#@мови cat::кіт\n\n#@мови dog::пес\n\n#@мови fox::лис\n")];
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    let (vault2, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,4,show,4,show,4")]);
    assert_eq!(review_line_of(&out), "done мови 3", "{out}");
    let note = std::fs::read_to_string(vault2.path().join("n.md")).unwrap();
    assert_eq!(note.matches(" ^c-").count(), 3, "each has its id, at its own line's end: {note}");
    assert!(note.lines().filter(|l| l.starts_with("#@")).all(|l| l.contains(" ^c-")), "{note}");
}

#[test]
fn a_deck_s_settings_are_not_written_over_a_file_that_cannot_be_read_as_one() {
    let notes = [("n.md", "#@мови cat::кіт ^c-1\n\n#@історія Коли?::Тоді ^c-2\n")];
    let (vault, _) = probe_notes(&notes, &[]);
    let file = vault.path().join(".knotes/decks.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    // A sync's conflict left in it: the other decks' settings are in there still.
    let damaged = "{\n<<<<<<< HEAD\n  \"історія\": {\"new_a_day\": 3}\n}\n";
    std::fs::write(&file, damaged).unwrap();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови"), ("KNOTES_PROBE_DECK_SETTINGS", "85|2|")]);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), damaged, "left as it was: {out}");
    assert!(out.contains("failed: Не вдалося прочитати налаштування колод"), "and said: {out}");
    // Mended, it is changed again, the other deck's settings kept.
    std::fs::write(&file, "{\"історія\": {\"new_a_day\": 3}}").unwrap();
    probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови"), ("KNOTES_PROBE_DECK_SETTINGS", "85|2|")]);
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.contains("\"історія\": {\n    \"new_a_day\": 3") && written.contains("\"мови\": {\n    \"new_a_day\": 2"), "{written}");
}

#[test]
fn the_last_grade_is_taken_back_and_the_card_graded_as_meant() {
    let notes = [("n.md", "#@мови cat::кіт ^c-1\n\n#@мови dog::пес ^c-2\n")];
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:мови")];
    // cat graded Good by a slip: taken back, it shows again with its answer, a new card as it
    // was, and dog waits its turn again (what the review keeps then is tested in knotes-app's
    // `review_session`). Graded Easy, as meant: its days begin, dog is next.
    let (vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,3,undo,4")]);
    assert_eq!(review_line_of(&out), "question мови 1/1 | dog (list hidden)", "{out}");
    // The log says all three: the grade, that it was taken back, the grade meant.
    let logs: Vec<_> = std::fs::read_dir(vault.path().join(".knotes/reviews")).unwrap().flatten().collect();
    let log = std::fs::read_to_string(logs[0].path()).unwrap();
    let grades: Vec<&str> = log.lines().map(|l| l.rsplit('\t').next().unwrap()).collect();
    assert_eq!(grades, ["3", "0", "4"], "{log}");
    // The next launch: cat stands as graded Easy alone (in sixteen days), dog new.
    let out = probe_env(&[vault.path()], None, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show")]);
    assert!(review_line_of(&out).starts_with("answer мови 0/1 | dog | пес | "), "{out}");
}

#[test]
fn what_is_selected_in_a_note_becomes_a_card() {
    let notes = [("n.md", "Кіт — це тварина.\n\nкінець")];
    // «тварина» (9..16) the answer to a question asked.
    let (vault, out) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_ACTION", "make-card=9,7"), ("KNOTES_PROBE_CARD", "біологія/звірі|Хто такий кіт?")]);
    let note = std::fs::read_to_string(vault.path().join("n.md")).unwrap();
    let id = note.split(" ^").nth(1).and_then(|rest| rest.split('\n').next()).unwrap_or_else(|| panic!("{note}")).to_owned();
    assert_eq!(note, format!("Кіт — це \n\n#@біологія/звірі Хто такий кіт?::тварина ^{id}\n\n.\n\nкінець"), "{out}");
    assert!(id.starts_with("c-") && id.len() == 8, "{id}");
    // Its deck is there the next time.
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SIDEBAR", "cards")]);
    assert!(out.contains("sidebar: Усі колоди (1) | біологія (1)\n"), "{out}");
    // Nothing selected, or no deck given: nothing is written.
    let (vault, _) = probe_notes(&notes, &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_ACTION", "make-card=9,7")]);
    assert_eq!(std::fs::read_to_string(vault.path().join("n.md")).unwrap(), notes[0].1);
}

#[test]
fn a_deck_s_review_is_a_tab_of_its_own_beside_the_documents() {
    let home = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in CARD_NOTES {
        std::fs::write(vault.path().join(path), text).unwrap();
    }
    // A note open, then a deck chosen: the note keeps its tab, the review gets one beside it.
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_OPEN", "Мови.md")]);
    assert!(out.contains("note: Мови.md"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:історія")]);
    assert!(out.contains("tabs: Мови | історія\n") && out.contains("tab-shown: історія\n"), "two tabs: {out}");
    assert_eq!(review_line_of(&out), "waiting історія 1", "{out}");
    assert!(out.contains("note-title: історія\n"), "the deck's name where a document's is: {out}");
    // The note's tab clicked: the note again, the review's tab kept; and back to the review.
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_TAB_CLICK", "Мови.md")]);
    assert!(out.contains("tabs: Мови | історія\n") && review_line_of(&out) == "hidden" && out.contains("note: Мови.md"), "{out}");
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_TAB_CLICK", "deck://історія")]);
    assert_eq!(review_line_of(&out), "waiting історія 1", "{out}");
    // The next launch comes back to the review's tab.
    let out = probe_env(&[vault.path()], Some(home.path()), &[("KNOTES_PROBE_REVIEW", "start")]);
    assert_eq!(review_line_of(&out), "question історія 0/1 | Коли була битва? (list hidden)", "{out}");
}

#[test]
fn a_card_in_braces_is_reviewed_with_its_paragraphs_and_its_links_open_their_notes() {
    let notes = [
        ("Битви.md", "Вступ.\n\n#@історія{\nКоли була битва?\n\nІ де саме?\n?\n1659 року.\n\nПід Конотопом, див. [[Конотоп]] і [сайт](https://example.com).\n}\n\nДалі."),
        ("Конотоп.md", "Місто на Сумщині."),
    ];
    let open = [("KNOTES_PROBE_SIDEBAR", "cards"), ("KNOTES_PROBE_SELECT", "d:історія")];
    let (_vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show")]);
    assert!(review_line_of(&out).starts_with("answer історія 0/1 | Коли була битва?⏎⏎І де саме? | 1659 року.⏎⏎Під Конотопом, див. [[Конотоп]] і [сайт](https://example.com). | Знову"), "both paragraphs of each: {out}");
    // Its first link clicked: the note it names, in a tab beside the review's.
    let (_vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,link0")]);
    assert!(out.contains("note: Конотоп.md") && out.contains("tabs: історія | Конотоп\n"), "{out}");
    // Graded: its id goes after its closing brace.
    let (vault, out) = probe_notes(&notes, &[open[0], open[1], ("KNOTES_PROBE_REVIEW", "start,show,3")]);
    let note = std::fs::read_to_string(vault.path().join("Битви.md")).unwrap();
    assert!(note.contains("\n} ^c-") && note.ends_with("\n\nДалі."), "{note} {out}");
}

#[test]
fn a_thematic_break_shows_as_a_line_and_the_frontmatter_small_and_muted() {
    let note = "---\naliases: [Кіт]\n---\n# Барсик\n\nПерший.\n\n---\n\nДругий.\n\n***\n\nкінець";
    let (_vault, out) = probe_notes(&[("n.md", note)], &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    // Both breaks are lines, their signs not shown; the frontmatter's fences are no breaks.
    assert_eq!(out.matches("decoration: rule ").count(), 2, "{out}");
    assert_eq!(runs.matches("[rule-line]").count(), 2, "{runs}");
    assert!(runs.contains("[frontmatter]---"), "the note's properties, with their fences, quiet: {runs}");
    // With the cursor in a break's paragraph its signs show, to be edited; the other stays a line.
    let (_vault, out) = probe_notes(&[("n.md", note)], &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", "43")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[rule]---") && out.matches("decoration: rule ").count() == 1 && runs.matches("[rule-line]").count() == 1, "{runs} {out}");
}

#[test]
fn a_card_s_id_shows_small_and_muted_in_the_note() {
    let note = "#@мови cat::кіт ^c-h0a1ek\n\n#@історія{\nКоли?\n?\nТоді.\n} ^c-zz9\n\nтекст ^c-не-картка\n\nкінець";
    let (_vault, out) = probe_notes(&[("n.md", note)], &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    assert!(runs.contains("[card-id]^c-h0a1ek|") && runs.contains("[card-id]^c-zz9|"), "{runs}");
    assert!(!runs.contains("[card-id]^c-не"), "only a card's: {runs}");
}

#[test]
fn a_card_shows_its_deck_on_a_plate_its_question_strong_and_its_marks_muted() {
    let note = "#@мови cat::кіт ^c-1\n\n#@історія{\nКоли?\n?\nТоді.\n} ^c-2\n\n#@біо Кіт — ==тварина==.\n\nне картка :: просто текст\n\nкінець";
    let (_vault, out) = probe_notes(&[("n.md", note)], &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    let runs = editor_runs(&out);
    for shown in ["[card-deck]#@мови|", "[card-question]cat|", "[card-mark]::|", "[card-deck]#@історія|", "[card-mark]{|", "[card-question]Коли?|", "[card-mark]?|", "[card-mark]}|", "[card-deck]#@біо|"] {
        assert!(runs.contains(shown), "{shown}: {runs}");
    }
    assert_eq!(runs.matches("[card-question]").count(), 2, "a cloze has no question apart: {runs}");
    assert_eq!(runs.matches("[card-mark]::").count(), 1, "only a card's: {runs}");
    // Each stands as a block, as a quote does, with a bar of the cards' colour down its lines
    // (the blank ones inside braces too): 1 + 5 + 1.
    assert_eq!(out.matches("decoration: card ").count(), 7, "{out}");
    assert!(!out.contains("decoration: quote "), "{out}");
    // A list item's card keeps its item's look, and a quote its own bar.
    let (_vault, out) = probe_notes(&[("n.md", "- #@мови cat::кіт\n\n> цитата\n\nкінець")], &[("KNOTES_PROBE_OPEN", "n.md"), ("KNOTES_PROBE_CURSOR", "end")]);
    assert!(editor_runs(&out).contains("[card-deck]#@мови|") && !out.contains("decoration: card ") && out.matches("decoration: quote ").count() == 1, "{out}");
}

#[test]
fn the_vaults_window_lists_the_vaults_opened_and_opens_or_forgets_one() {
    let home = tempfile::tempdir().unwrap();
    let name = |vault: &tempfile::TempDir| vault.path().file_name().unwrap().to_string_lossy().into_owned();
    let line = |out: &str, name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or_else(|| panic!("{name}: {out}")).to_owned();
    // Two vaults opened one after the other: both are known, the last opened first and marked
    // as the one a window shows.
    let (first, _) = probe_notes_at_home(&[("a.md", "a")], &[], home.path());
    let (second, out) = probe_notes_at_home(&[("b.md", "b")], &[("KNOTES_PROBE_VAULTS", "1")], home.path());
    let listed = line(&out, "vaults: ");
    let rows: Vec<&str> = listed.split(" | ").collect();
    assert!(rows.len() == 2 && rows[0].starts_with(&format!("{} (open) — ", name(&second))) && rows[1].starts_with(&format!("{} — ", name(&first))), "{listed}");
    // A click on the other one opens it in a window of its own: it is the first now.
    let out = probe_env(&[second.path()], Some(home.path()), &[("KNOTES_PROBE_VAULTS", "open:1")]);
    let acted = line(&out, "vaults-acted: ");
    assert!(acted.starts_with(&format!("{} (open) — ", name(&first))) && acted.contains(&format!(" | {} (open) — ", name(&second))) && acted.ends_with("| windows: 2"), "{acted}");
    // Taken off the list, a vault is listed no more; its folder stays.
    let out = probe_env(&[second.path()], Some(home.path()), &[("KNOTES_PROBE_VAULTS", "forget:1")]);
    let acted = line(&out, "vaults-acted: ");
    assert!(acted.starts_with(&format!("{} (open) — ", name(&second))) && !acted.contains(&name(&first)) && acted.ends_with("| windows: 1"), "{acted}");
    assert!(first.path().join("a.md").is_file());
}

#[test]
fn an_inline_run_shows_its_signs_only_with_the_cursor_in_it() {
    let note = "# Заголовок\n\nПерше **жирне** і *курсив*, формула $x^2$ і `код`.\n\nкінець";
    let at = |find: &str| note.chars().take(note.char_indices().position(|(i, _)| i == note.find(find).unwrap()).unwrap()).map(char::len_utf16).sum::<usize>();
    let runs = |cursor: usize| {
        let (_vault, out) = probe_notes(&[("a.md", note)], &[("KNOTES_PROBE_OPEN", "a.md"), ("KNOTES_PROBE_CURSOR", &cursor.to_string())]);
        (editor_runs(&out), out)
    };
    // The cursor in the line, in none of its runs: the line reads as it will.
    let (line, _) = runs(at("Перше"));
    assert!(line.contains("|[hidden]**|[strong]жирне|[hidden]**|") && line.contains("|[hidden]*|[emphasis]курсив|[hidden]*|") && line.contains("|[hidden]`|[code]код|[hidden]`|"), "{line}");
    // In the bold run, or at an end of it: its signs, and no other run's.
    for cursor in [at("**жирне"), at("жирне") + 2, at("жирне") + 7] {
        let (line, _) = runs(cursor);
        assert!(line.contains("|[markup]**|[strong]жирне|[markup]**|") && line.contains("|[hidden]*|[emphasis]курсив|[hidden]*|"), "{cursor}: {line}");
    }
    // A formula in the line: typeset until the cursor is in it.
    let (_, out) = runs(at("Перше"));
    assert_eq!(out.lines().filter(|l| l.starts_with("formula-image: ")).count(), 1, "{out}");
    let (_, out) = runs(at("$x^2$") + 2);
    assert_eq!(out.lines().filter(|l| l.starts_with("formula-image: ")).count(), 0, "its source shows: {out}");
    // A heading's sign is its line's: it shows with the cursor anywhere in the line.
    let (line, _) = runs(4);
    assert!(line.starts_with("[markup]# |"), "{line}");
}
