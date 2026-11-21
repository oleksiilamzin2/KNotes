//! Probe mode (`KNOTES_PROBE`): what the variables ask of the window once its vault is shown,
//! as the user would do it, before the report.

use super::*;

impl VaultWindow {
    /// Does what the probe's variables ask, once, the first snapshot with notes shown.
    pub(super) fn act_as_probe_asks(&self) {
        probe::sidebar_mode_requested(&self.window);
        probe::search_requested(&self.window);
        // `KNOTES_PROBE_SEARCH_CASE=1`: a click on the search row's «Aa» (the case kept).
        if std::env::var_os("KNOTES_PROBE_SEARCH_CASE").is_some() {
            self.search_bar.click_case();
        }
        probe::toggle_sidebar_requested(&self.window);
        probe::panel_requested(&self.window);
        // `KNOTES_PROBE_ATTACHMENTS=all|unlinked|missing`: the attachments' window with that
        // filter; `KNOTES_PROBE_ATTACHMENTS_ACT=0,2`: those rows chosen, the right button clicked.
        if let Ok(filter) = std::env::var("KNOTES_PROBE_ATTACHMENTS") {
            self.show_attachments();
            if let Some(window) = self.attachments.borrow().as_ref() {
                window.probe_filter(match filter.as_str() {
                    "unlinked" => knotes_app::AttachmentFilter::Unlinked,
                    "missing" => knotes_app::AttachmentFilter::Missing,
                    _ => knotes_app::AttachmentFilter::All,
                });
                // `KNOTES_PROBE_ATTACHMENTS_HOVER=0`: the pointer over that row.
                if let Some(row) = std::env::var("KNOTES_PROBE_ATTACHMENTS_HOVER").ok().and_then(|row| row.parse().ok()) {
                    println!("attachment-preview: {}", window.probe_hover(row));
                }
                if let Ok(rows) = std::env::var("KNOTES_PROBE_ATTACHMENTS_ACT") {
                    window.probe_act(&rows.split(',').filter_map(|row| row.parse().ok()).collect::<Vec<usize>>());
                }
            }
        }
        // `KNOTES_PROBE_ACTION=zen`: the zen button; `zen-twice`: and back once it has gone.
        let action = std::env::var("KNOTES_PROBE_ACTION").unwrap_or_default();
        if action == "zen" || action == "zen-twice" {
            self.toggle_zen();
            if action == "zen-twice" {
                let weak = self.this.borrow().clone();
                let when = dispatch2::DispatchTime::try_from(std::time::Duration::from_millis(700)).expect("short delay");
                let weak = dispatch2::MainThreadBound::new(weak, objc2::MainThreadOnly::mtm(&*self.window));
                let _ = dispatch2::DispatchQueue::main().after(when, move || {
                    let mtm = objc2::MainThreadMarker::new().expect("main queue runs on the main thread");
                    if let Some(window) = weak.get(mtm).upgrade() {
                        window.toggle_zen();
                    }
                });
            }
        }
        if let Some(space) = probe::requested_space() {
            // Through the switcher, as a click.
            self.space_switcher.probe_choose(space.as_deref());
        }
        // How long a step took, said when `KNOTES_PROBE_TIMES` is set: to measure the app on a
        // large vault.
        let timed = |what: &str, began: std::time::Instant| {
            if std::env::var_os("KNOTES_PROBE_TIMES").is_some() {
                println!("took: {what} {:.1} ms", began.elapsed().as_secs_f64() * 1000.0);
            }
        };
        if let Some(item) = probe::requested_selection() {
            let began = std::time::Instant::now();
            self.highlight(&item);
            self.select(item);
            timed("a row chosen", began);
        }
        // `KNOTES_PROBE_ACTION=empty-button`: a click on the button over the recently deleted.
        let empty = toolbar::row_control(&self.window, toolbar::EMPTY_DELETED).and_then(|v| v.downcast::<objc2_app_kit::NSButton>().ok());
        if let Some(button) = empty.filter(|b| !b.isHidden() && std::env::var("KNOTES_PROBE_ACTION").as_deref() == Ok("empty-button")) {
            // SAFETY: a click on a button, as the user's.
            unsafe { button.performClick(None) };
        }
        // `KNOTES_PROBE_DECK_SETTINGS=85|10`: the chosen deck's menu → its settings, so typed.
        let chosen = self.review_deck.borrow().clone();
        if std::env::var_os("KNOTES_PROBE_DECK_SETTINGS").is_some()
            && let Some(deck) = chosen
        {
            self.set_deck(&deck);
        }
        // `KNOTES_PROBE_DECK_TABLE=мови` (`""`: all the decks): the deck's menu → «Таблиця
        // карток»; `KNOTES_PROBE_DECK_TABLE_SORT=due,due`: clicks on columns' names;
        // `KNOTES_PROBE_DECK_TABLE_EDIT=1=new text`: that row chosen, its card's text typed
        // over and ⌘S.
        if let Ok(deck) = std::env::var("KNOTES_PROBE_DECK_TABLE") {
            let began = std::time::Instant::now();
            self.show_deck_table(&deck);
            timed("the table shown", began);
            if let Some(table) = self.deck_table().as_ref() {
                // `KNOTES_PROBE_DECK_TABLE_FIND=cat`: typed in the table's search.
                if let Ok(query) = std::env::var("KNOTES_PROBE_DECK_TABLE_FIND") {
                    let began = std::time::Instant::now();
                    table.probe_find(&query);
                    timed("the table searched", began);
                }
                for column in std::env::var("KNOTES_PROBE_DECK_TABLE_SORT").unwrap_or_default().split(',').filter(|c| !c.is_empty()) {
                    let began = std::time::Instant::now();
                    table.probe_sort(column);
                    timed("the table sorted", began);
                }
                println!("deck-table: {}", table.describe());
                // `KNOTES_PROBE_DECK_TABLE_ACT=1=pause;0=deck;2=delete`: rows' menu items in turn
                // (`deck` takes `KNOTES_PROBE_CARD=name|`).
                if let Ok(acts) = std::env::var("KNOTES_PROBE_DECK_TABLE_ACT") {
                    for (row, act) in acts.split(';').filter_map(|act| act.split_once('=')) {
                        // `0+2=pause`: the rows chosen together.
                        let rows: Vec<usize> = row.split('+').filter_map(|row| row.parse().ok()).collect();
                        table.probe_act(&rows, act);
                    }
                    println!("deck-table-acted: {}", table.describe());
                }
                if let Some((row, text)) = std::env::var("KNOTES_PROBE_DECK_TABLE_EDIT").ok().as_deref().and_then(|spec| spec.split_once('=')) {
                    let written = table.probe_edit(row.parse().unwrap_or(0), &text.replace("\\n", "\n"));
                    println!("deck-table-edit: {}", if written { "written" } else { "not written" });
                    println!("deck-table-after: {}", table.describe());
                }
            }
        }
        // `KNOTES_PROBE_REVIEW=start,show,3`: clicks in a deck's review (ADR 0037).
        if let Ok(clicks) = std::env::var("KNOTES_PROBE_REVIEW") {
            let began = std::time::Instant::now();
            self.probe_review(&clicks);
            timed("the review's clicks", began);
        }
        if let Some(option) = probe::requested_option() {
            self.choose(option);
        }
        if probe::new_note_requested() {
            probe::type_requested(&self.note_text);
        }
        for (i, path) in probe::requested_notes().iter().enumerate() {
            if let Some(row) = self.list.row_of(path) {
                // Selecting the row goes through the same delegate path as a click.
                self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false);
                // Shown before the next one opens, as on screen.
                self.window.displayIfNeeded();
            } else if FileKind::from_path(path) == FileKind::Pdf {
                // A PDF is listed only among a search's results, which come later: opened
                // as a click on its result opens it.
                self.open_note(path, true);
                self.window.displayIfNeeded();
            }
            if i == 0 {
                probe::type_requested(&self.note_text);
                probe::keys_requested(&self.note_text);
                if let Some(row) = std::env::var("KNOTES_PROBE_COMPLETION_CLICK").ok().and_then(|r| r.parse().ok()) {
                    self.completion.click_row(row);
                }
            }
        }
        probe::save_requested();
        probe::write_requested(self);
        probe::rewrite_image_requested(self);
        if let (Some(name), Some(open)) = (probe::rename_requested(), self.open_path()) {
            self.rename_note(&open, &name);
        }
        // A list row renamed, or dropped on a sidebar folder, by its path
        // (`list-rename-path=Docs/a.pdf=New name`, `drop-path=Docs/a.pdf=Folder`): a PDF is listed
        // only among a search's results, which come later; these reach what its row calls.
        // `KNOTES_PROBE_DELETE_OPEN=1`: the note open deleted, as its row's «Видалити» does,
        // after what was typed in it.
        if std::env::var_os("KNOTES_PROBE_DELETE_OPEN").is_some()
            && let Some(path) = self.open_path()
        {
            self.delete(&path);
        }
        if let Ok(action) = std::env::var("KNOTES_PROBE_ACTION") {
            if let Some((path, name)) = action.strip_prefix("list-rename-path=").and_then(|s| s.split_once('=')) {
                self.rename_note(path, name);
            }
            if let Some((path, folder)) = action.strip_prefix("drop-path=").and_then(|s| s.split_once('=')) {
                self.move_note_to(path, folder);
            }
            // `make-card=6,7`: those characters of the open note (UTF-16: where, how many)
            // selected, then Edit → Make a Card (its deck and question from `KNOTES_PROBE_CARD`).
            if let Some((at, len)) = action.strip_prefix("make-card=").and_then(|r| r.split_once(',')).and_then(|(a, l)| Some((a.parse().ok()?, l.parse().ok()?))) {
                self.note_text.setSelectedRange(NSRange::new(at, len));
                self.make_card();
            }
            // `path-bar=1`: a click on that item of the path bar (0: the vault).
            if let Some(index) = action.strip_prefix("path-bar=").and_then(|i| i.parse().ok()) {
                self.path_bar.click(index);
            }
            // `ai-path=Docs/a.pdf`: what the row's «Сховати від ШІ» calls (ADR 0036);
            // `delete-path=Docs/a.pdf`: what its «Видалити» calls.
            if let Some(path) = action.strip_prefix("ai-path=") {
                self.toggle_ai(path);
            }
            if let Some(path) = action.strip_prefix("delete-path=") {
                self.delete(path);
            }
        }
        probe::list_menu_requested(&self.list, &self.table);
        probe::drag_requested(&self.list, &self.table, &self.sidebar);
        probe::folder_rename_requested(&self.sidebar, &self.outline);
        probe::folder_space_requested(&self.sidebar);
        probe::new_folder_requested(&self.sidebar, &self.outline);
        probe::folder_delete_requested(&self.sidebar, &self.outline);
        if let Some(forward) = probe::requested_navigation() {
            self.navigate(forward);
        }
        if let Some(width) = std::env::var("KNOTES_PROBE_WIDTH").ok().and_then(|w| w.parse::<f64>().ok()) {
            let height = self.window.frame().size.height;
            self.window.setContentSize(objc2_foundation::NSSize::new(width, height));
            self.window.layoutIfNeeded();
        }
        if let Ok(cursor) = std::env::var("KNOTES_PROBE_CURSOR") {
            let end = self.note_text.string().length();
            let at = if cursor == "end" { end } else { cursor.parse().unwrap_or(0).min(end) };
            self.note_text.setSelectedRange(objc2_foundation::NSRange::new(at, 0));
        }
        probe::paste_requested(&self.note_text);
        probe::image_edge_requested(&self.note_text);
        probe::image_hover_requested(&self.note_text);
        probe::pointer_requested(&self.note_text);
        if let Ok(scroll) = std::env::var("KNOTES_PROBE_TABLE_SCROLL") {
            probe::table_scroll_requested(&self.note_text, &scroll);
        }
        if let Ok(spec) = std::env::var("KNOTES_PROBE_TABLE_CONTEXT_AT") {
            self.live_preview.probe_table_context_at(&self.note_text, &spec);
        }
        if let Ok(spec) = std::env::var("KNOTES_PROBE_TABLE_CONTEXT") {
            self.live_preview.probe_table_context(&self.note_text, &spec);
        }
        if let Ok(press) = std::env::var("KNOTES_PROBE_TABLE_PRESS") {
            self.live_preview.probe_table_press(&self.note_text, &press);
        }
        if let Ok(ranges) = std::env::var("KNOTES_PROBE_RANGE_X") {
            probe::report_range_xs(&self.note_text, &ranges);
        }
        if let Ok(ranges) = std::env::var("KNOTES_PROBE_SPELLING") {
            let answers: Vec<&str> = ranges
                .split(';')
                .filter_map(|r| r.split_once(','))
                .filter_map(|(a, b)| Some(NSRange::new(a.parse().ok()?, b.parse().ok()?)))
                .map(|range| if self.live_preview.spelling_hidden(range) { "dropped" } else { "kept" })
                .collect();
            println!("spelling: {}", answers.join(" "));
        }
        if let Some(line) = std::env::var("KNOTES_PROBE_QUICK_LOOK").ok().and_then(|l| l.parse().ok()) {
            // The file the panel would show; the panel itself is not opened in probe mode.
            println!("quick-look: {}", self.note_text.image_file(line).map(|f| f.display().to_string()).unwrap_or_default());
        }
        if let Some((line, width)) = probe::resize_requested() {
            self.live_preview.resize_image(&self.note_text, line, width, true);
        }
        probe::menu_requested();
        if std::env::var("KNOTES_PROBE_SHORTCUT_FOCUS").as_deref() == Ok("list") {
            probe::shortcut_requested(&self.window, &self.table);
        } else {
            probe::shortcut_requested(&self.window, &self.note_text);
        }
        if let Ok(word) = std::env::var("KNOTES_PROBE_FIND") {
            self.open_find();
            self.find_bar.set_text(&word);
            self.find_text(&word, true);
            // `KNOTES_PROBE_FIND_CASE=1`: a click on the bar's «Aa» (the case kept from then on).
            if std::env::var_os("KNOTES_PROBE_FIND_CASE").is_some() {
                self.find_bar.click_case();
            }
            let steps: usize = std::env::var("KNOTES_PROBE_FIND_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            for _ in 0..steps {
                self.find_step(true);
            }
        }
        probe::click_requested(&self.note_text);
        // The click opened a PDF: the word is found in it.
        if let (Ok(word), true) = (std::env::var("KNOTES_PROBE_FIND"), self.pdf_view.borrow().is_some()) {
            self.open_find();
            self.find_bar.set_text(&word);
            self.find_text(&word, true);
        }
        probe::mouse_drag_requested(&self.note_text);
        self.live_preview.probe_code_pointer(&self.note_text);
        if let Ok(spec) = std::env::var("KNOTES_PROBE_LINK_MENU") {
            self.live_preview.probe_link_menu(&spec);
        }
        // `KNOTES_PROBE_NEW_TAB=b.md`: that note opened in a new tab, as the list's menu does.
        if let Ok(path) = std::env::var("KNOTES_PROBE_NEW_TAB") {
            self.open_in_new_tab(&path);
        }
        probe::toolbar_button_requested(&self.window);
        // `KNOTES_PROBE_VAULT_SEARCH=radar budget` (with `KNOTES_PROBE_VAULT_SEARCH_DOWN=1`, and
        // `KNOTES_PROBE_VAULT_SEARCH_OPEN=1` for Return): the vault's search window.
        if let Ok(query) = std::env::var("KNOTES_PROBE_VAULT_SEARCH") {
            let down = std::env::var("KNOTES_PROBE_VAULT_SEARCH_DOWN").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
            self.probe_search_vault(&query, down, std::env::var_os("KNOTES_PROBE_VAULT_SEARCH_OPEN").is_some());
        }
        // `KNOTES_PROBE_TAB_CLICK=a.md`: a click on that document's tab, after the opening.
        if let Ok(path) = std::env::var("KNOTES_PROBE_TAB_CLICK") {
            if let Some(tab) = self.tab_strip.tab_of(&path) {
                self.tab_strip.click(tab);
            }
        }
        // After the click: it may open a PDF to rename.
        probe::toolbar_rename_requested(&self.window);
        probe::title_clicks_requested(&self.window);
        if let Ok(title) = std::env::var("KNOTES_PROBE_OUTLINE_CLICK") {
            self.window.displayIfNeeded();
            self.outline_panel.click(&title);
        }
        if let Ok(title) = std::env::var("KNOTES_PROBE_OUTLINE_FOLD") {
            self.outline_panel.fold(&title);
        }

        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            probe::pdf_mouse_requested(viewer);
            probe::pdf_copy_requested(viewer);
        }
        if let Some(this) = self.this.borrow().upgrade() {
            probe::pdf_width_requested(&this);
            probe::connection_link_requested(&this);
        }
        if let Some(page) = std::env::var("KNOTES_PROBE_PDF_GO").ok().and_then(|p| p.parse().ok()) {
            self.window.displayIfNeeded();
            self.pdf_go_to(page);
        }
        // `KNOTES_PROBE_PDF_SCROLLER=3`: the scroller dragged until page 3 is at the top.
        if let Some(page) = std::env::var("KNOTES_PROBE_PDF_SCROLLER").ok().and_then(|p| p.parse::<usize>().ok()) {
            if let Some(viewer) = self.pdf_view.borrow().as_ref() {
                self.window.displayIfNeeded();
                viewer.scroll_as_scroller(page.saturating_sub(1));
            }
        }
        if std::env::var("KNOTES_PROBE_AFTER_CLICK").as_deref() == Ok("back") {
            // Shown first, as on screen.
            self.window.displayIfNeeded();
            self.navigate(false);
        }
        println!("found-in-note: {}", self.live_preview.found_count());
        if std::env::var("KNOTES_PROBE_APPEARANCE").as_deref() == Ok("dark") {
            // SAFETY: AppKit's appearance name constant lives for the whole process.
            objc2_app_kit::NSAppearanceCustomization::setAppearance(&*self.window, objc2_app_kit::NSAppearance::appearanceNamed(unsafe { objc2_app_kit::NSAppearanceNameDarkAqua }).as_deref());
        }
        if let Some((button, text)) = std::env::var("KNOTES_PROBE_HOVER_EMBED").ok().and_then(|n| n.parse().ok()).and_then(|n| self.embeds.probe_points(n)) {
            let in_window = |p| self.note_text.convertPoint_toView(p, None);
            println!("hover-embed-open: {}", self.note_text.pointer(in_window(button)));
            println!("hover-embed-text: {}", self.note_text.pointer(in_window(text)));
        }
        if let Some(at) = std::env::var("KNOTES_PROBE_HOVER").ok().and_then(|a| a.parse().ok()) {
            println!("hover: {}", if self.live_preview.clickable(at) { "hand" } else { "text" });
        }
        probe::note_scroll_requested(&self.note_text);
        probe::chat_requested(&self.chat);
        probe::schedule(true);
    }
}
