//! The right panel (ADR 0023, 0026): the outline and the connections of the document open, the chat's switch.

use super::*;

impl VaultWindow {
    /// The AI or the outline in the right panel (the toolbar switch).
    pub fn set_panel_mode(&self, mode: PanelMode) {
        self.toolbar_delegate.set_panel_mode(mode);
        self.show_panel(mode);
    }

    pub(super) fn show_panel(&self, mode: PanelMode) {
        self.panel_mode.set(mode);
        self.chat.view.setHidden(mode != PanelMode::Ai);
        self.outline_panel.view.setHidden(mode != PanelMode::Outline);
        self.connections_panel.view.setHidden(mode != PanelMode::Links);
        self.stars_panel.view.setHidden(mode != PanelMode::Stars);
        self.refresh_stars();
        self.refresh_panel();
    }

    /// The right panel's outline or connections for the document open.
    pub(super) fn refresh_panel(&self) {
        self.chat.set_document(self.pdf_path.borrow().clone().or_else(|| self.open_path()));
        self.refresh_outline();
        self.refresh_connections();
    }

    /// Lists the connections of the document open, when the panel shows them (ADR 0026): its
    /// own links now, from the editor's text; the notes linking to it (from the index's
    /// backlinks) and naming it (from a full-text search of its name), with their lines, once
    /// gathered on another thread. Another document's list is cleared at once; the same
    /// document's stays until the new one comes.
    pub(super) fn refresh_connections(&self) {
        if self.panel_mode.get() != PanelMode::Links {
            return;
        }
        self.connections_job.borrow().store(true, std::sync::atomic::Ordering::Relaxed);
        let document = self.pdf_path.borrow().clone().or_else(|| self.open_path());
        let (vault, index_dir, links) = {
            let state = self.state.borrow();
            (state.vault.clone(), state.index_dir.clone(), state.links.clone())
        };
        let (Some(document), Some(vault), Some(index_dir), Some(links), Some(mtm)) = (document, vault, index_dir, links, MainThreadMarker::new()) else {
            *self.connections_of.borrow_mut() = None;
            self.connections_panel.show(Connections::default());
            return;
        };
        let outgoing = self.outgoing_of(&document);
        if self.connections_of.borrow().as_deref() != Some(document.as_str()) {
            *self.connections_of.borrow_mut() = Some(document.clone());
            self.connections_panel.forget_more();
            self.connections_panel.show(Connections { outgoing, ..Connections::default() });
        } else {
            self.connections_panel.set_outgoing(outgoing);
        }
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        *self.connections_job.borrow_mut() = std::sync::Arc::clone(&cancelled);
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let name = document_name(&document).to_owned();
        std::thread::spawn(move || {
            let found = knotes_assistant::connections::gather(&vault, &index_dir, &document, &name, &links, &cancelled);
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                // A newer list asked for, or another document: this one is stale.
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let (Some(window), Some((linked, unlinked))) = (target.get(mtm).upgrade(), found)
                    && window.connections_of.borrow().as_deref() == Some(document.as_str())
                {
                    window.connections_panel.set_mentions(linked, unlinked);
                }
            });
        });
    }

    /// The document's own links (a note's, as typed): only they change as it is edited.
    pub(super) fn refresh_outgoing(&self) {
        if self.panel_mode.get() != PanelMode::Links {
            return;
        }
        if let Some(document) = self.connections_of.borrow().clone() {
            self.connections_panel.set_outgoing(self.outgoing_of(&document));
        }
    }

    /// The links of `document` when it is the note open, as the editor holds it.
    pub(super) fn outgoing_of(&self, document: &str) -> Vec<knotes_app::Outgoing> {
        let Some(links) = self.state.borrow().links.clone() else { return Vec::new() };
        match self.open_path() {
            Some(note) if note == document => {
                let text = self.note_text.string().to_string();
                knotes_app::outgoing_links(&note, &text, &MarkdownParser::new().parse(&text), &*links)
            }
            _ => Vec::new(),
        }
    }

    /// A line in the connections clicked: its note opened in the tab shown, the link at
    /// `range` (bytes) selected, in view and pointed at.
    pub(super) fn open_connection(&self, path: &str, range: Option<std::ops::Range<usize>>) {
        if !self.open_note(path, true) {
            return;
        }
        let Some(range) = range else { return };
        let text = self.note_text.string().to_string();
        if range.end > text.len() || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            return;
        }
        let units = utf16_offsets(&text, &[range.start, range.end]);
        let selected = NSRange::new(units[0], units[1] - units[0]);
        self.note_text.setSelectedRange(selected);
        self.note_text.scrollRangeToVisible(selected);
        self.note_text.showFindIndicatorForRange(selected);
    }

    /// An outgoing link in the connections clicked: its note opened, or made when it is not
    /// there yet, as a click on the link does.
    pub(super) fn follow_connection(&self, label: &str, resolved: Option<String>) {
        match resolved {
            Some(path) => {
                self.open_note(&path, true);
            }
            None => self.follow_link(&Link {
                kind: LinkKind::Wiki,
                embed: false,
                target: label.to_owned(),
                subpath: None,
                display: None,
                span: knotes_core::Span { start: 0, end: 0 },
            }),
        }
    }

    /// An unlinked mention's button: the mention at `range` (bytes) in the note at `source`
    /// made a link to the document open, its words kept as the link's text.
    pub(super) fn make_connection_link(&self, source: &str, range: std::ops::Range<usize>) {
        let document = self.pdf_path.borrow().clone().or_else(|| self.open_path());
        let (Some(document), Some(vault), Some(links)) = (document, self.vault(), self.state.borrow().links.clone()) else { return };
        let files = FsVault::new(&vault);
        // A note not in UTF-8 is left as it is: written back, it would lose its bytes.
        let Ok(text) = files.read_utf8(source) else { return };
        // The note changed since: nothing is made.
        let name = document_name(&document);
        if !text.get(range.clone()).is_some_and(|words| words.eq_ignore_ascii_case(name) || words.to_lowercase() == name.to_lowercase()) {
            return;
        }
        let new = knotes_app::make_link(&text, range, &links.link_text(&document, source));
        if let Err(e) = files.write_note(source, &new) {
            self.tell_failed(&strings::mention_failed(document_name(source)), &e);
        }
        self.refresh_connections();
    }

    /// Lists the headings of the document open in the outline, when the panel shows it.
    pub(super) fn refresh_outline(&self) {
        if self.panel_mode.get() != PanelMode::Outline {
            return;
        }
        let pdf = self.pdf_path.borrow().clone();
        let note = self.editing.borrow().as_ref().map(|e| e.path.clone());
        let (outline, empty) = if let Some(path) = &pdf {
            let bookmarks = self.vault().and_then(|v| knotes_pdf::pdf_outline(&v.join(path))).unwrap_or_default();
            let entries = bookmarks.into_iter().map(|b| OutlineEntry { level: b.level, title: b.title, place: (b.page, b.y) }).collect();
            (Outline::new(entries), strings::NO_PDF_OUTLINE)
        } else if note.is_some() {
            let text = self.note_text.string().to_string();
            let headings = note_headings(&text);
            let offsets = utf16_offsets(&text, &headings.iter().map(|h| h.span.start as usize).collect::<Vec<_>>());
            let entries = headings.into_iter().zip(offsets).map(|(h, at)| OutlineEntry { level: h.level, title: h.text, place: (at, 0.0) }).collect();
            (Outline::new(entries), strings::NO_HEADINGS)
        } else {
            (Outline::default(), "")
        };
        let document = pdf.or(note);
        let same = *self.outline_of.borrow() == document;
        *self.outline_of.borrow_mut() = document;
        self.outline_panel.show(outline, empty, same);
        self.outline_follow();
    }

    /// Scrolls the document open to heading `heading` of the outline (a click on it).
    pub(super) fn go_to_heading(&self, heading: usize) {
        let Some(entry) = self.outline_panel.entry(heading) else { return };
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.go_to_place(entry.place.0, entry.place.1);
        } else if self.editing.borrow().is_some() {
            self.scroll_note_to(entry.place.0);
        }
        self.outline_jump.set(self.document_scroll().map(|(y, _)| (heading, y)));
        self.outline_follow();
    }

    /// How far the document open is scrolled, and the place at the top of its view.
    pub(super) fn document_scroll(&self) -> Option<(f64, knotes_app::OutlinePlace)> {
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            return Some((viewer.scroll_y(), viewer.top_place()));
        }
        let y = self.note_text.enclosingScrollView()?.contentView().bounds().origin.y;
        Some((y, (self.note_top()?, 0.0)))
    }

    /// Highlights in the outline the heading of the part of the document in view: the one
    /// clicked last while the document stays where that put it.
    pub(super) fn outline_follow(&self) {
        if self.panel_mode.get() != PanelMode::Outline {
            return;
        }
        let Some((y, top)) = self.document_scroll() else { return self.outline_panel.set_current(None) };
        let current = match self.outline_jump.get() {
            Some((heading, at)) if (at - y).abs() < 0.5 => Some(heading),
            _ => {
                self.outline_jump.set(None);
                self.outline_panel.current_at(top)
            }
        };
        self.outline_panel.set_current(current);
    }

    /// An edit: the outline is listed again once typing pauses briefly (typing stays as fast).
    pub(super) fn outline_later(&self) {
        if self.panel_mode.get() == PanelMode::Ai || self.outline_due.replace(true) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let when = DispatchTime::try_from(OUTLINE_DELAY).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(window) = target.get(mtm).upgrade() {
                window.outline_due.set(false);
                // Typing changes the outline and the note's own links, not who links to it.
                window.refresh_outline();
                window.refresh_outgoing();
            }
        });
    }

    /// What the right panel shows, and the view showing (probe mode).
    pub fn describe_panel(&self) -> String {
        let shows = if !self.chat.view.isHidden() {
            "chat"
        } else if !self.outline_panel.view.isHidden() {
            "outline"
        } else if !self.connections_panel.view.isHidden() {
            "links"
        } else if !self.stars_panel.view.isHidden() {
            "stars"
        } else {
            "nothing"
        };
        format!("{}, shows {shows}", self.panel_mode.get().key())
    }

    /// Zen mode (View → Zen Mode, the toolbar's button): the note alone, the sidebar, the list
    /// and the panel folded away; again, and each is as it was before.
    pub fn toggle_zen(&self) {
        let mtm = MainThreadMarker::new().expect("the window lives on the main thread");
        let items = [&self.sidebar_item, &self.list_item, &self.panel_item];
        match self.zen.take() {
            None => {
                let width = |item: &NSSplitViewItem| item.viewController(mtm).view().frame().size.width;
                self.zen.set(Some((items.map(|item| item.isCollapsed()), [width(items[0]), width(items[1])])));
                items.iter().for_each(|item| item.setCollapsed(true));
            }
            Some((before, widths)) => {
                (0..3).for_each(|i| items[i].setCollapsed(before[i]));
                // Folded together, the sidebar and the list come back at their narrowest: each
                // gets its width again, once both are in place.
                if let Some(split) = self.window.contentViewController().and_then(|c| c.downcast::<objc2_app_kit::NSSplitViewController>().ok()).map(|c| c.splitView()) {
                    let split = MainThreadBound::new(split, mtm);
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                        let split = split.get(mtm);
                        let sidebar = if before[0] { 0.0 } else { widths[0] };
                        if !before[0] {
                            split.setPosition_ofDividerAtIndex(sidebar, 0);
                        }
                        if !before[1] {
                            split.setPosition_ofDividerAtIndex(sidebar + widths[1], 1);
                        }
                    });
                }
            }
        }
        let on = self.zen.get().is_some();
        // The toolbar's buttons go too (⌃⌥Z brings them back): the note's name alone on top.
        if let Some(toolbar) = self.window.toolbar() {
            toolbar.setVisible(!on);
        }
        self.toolbar_delegate.set_zen(on);
        // The keys go to the note: a folded column that had them left them with the window,
        // from where the sidebar's and the panel's buttons reach nothing.
        self.window.makeFirstResponder(Some(&self.note_text));
        // Nor the path under the note.
        self.find_bar.set_bottom(if on { 0.0 } else { crate::path_bar::HEIGHT });
        self.update_navigation();
    }

    /// Called when the user flips the AI/outline switch (the app saves the choice).
    /// The right panel shown (unfolded if folded away) with the chat, its question field
    /// focused (View → Ask the AI, Cmd+L).
    pub fn ask_ai(&self) {
        if self.panel_item.isCollapsed() {
            self.panel_item.setCollapsed(false);
        }
        if self.panel_mode.get() != PanelMode::Ai {
            self.set_panel_mode(PanelMode::Ai);
        }
        self.chat.focus();
    }

    pub fn on_panel_mode(&self, callback: impl Fn(PanelMode) + 'static) {
        let weak = self.this.borrow().clone();
        self.toolbar_delegate.on_panel_mode(move |mode| {
            if let Some(window) = weak.upgrade() {
                window.show_panel(mode);
            }
            callback(mode);
        });
    }
}
