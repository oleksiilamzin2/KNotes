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
    assert_eq!(sidebar, "Усі нотатки (3) | Теки | Inbox (1) | Projects (2) | Теги | radar (1) | todo (1)");
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
        "toggle-sidebar, sidebar-separator, flexible-space, list-options, list-separator, new-note, history(2), flexible-space, inspector-separator, chat"
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

    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "f:Projects")]);
    assert!(out.contains("title: Projects") && out.contains("subtitle: 2 нотатки"), "{out}");
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
    assert_eq!(menu, "Сортувати за > [✓ Датою зміни, Назвою] | ✓ Групувати за датою | ✓ Нотатки з підтек");
}

#[test]
fn choosing_a_sort_order_reorders_and_saves_only_that_field() {
    let vault = projects_vault();
    let out = probe_env(&[vault.path()], None, &[("KNOTES_PROBE_SELECT", "f:Projects"), ("KNOTES_PROBE_ACTION", "sort=title")]);
    assert_eq!(list_titles(&out), ["a note", "b note"]);
    assert!(out.contains("list-menu: Сортувати за > [Датою зміни, ✓ Назвою]"), "{out}");
    let saved = std::fs::read_to_string(vault.path().join("Projects/.knotes")).unwrap();
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
    assert!(out.contains("file-menu: Відкрити базу… ⌘O | Закрити вікно ⌘W"), "{out}");
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
