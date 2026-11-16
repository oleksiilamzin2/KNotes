//! The note column's documents: a note or a PDF opened and shown, the column as it should be, back and forward.

use super::*;

impl VaultWindow {
    /// Scrolls the note so the line at `offset` (UTF-16) is at the top, under the toolbar (as
    /// far as the note goes).
    pub(super) fn scroll_note_to(&self, offset: usize) {
        let view = &self.note_text;
        // Laid out around it first: TextKit only estimates where lines out of view are.
        view.scrollRangeToVisible(NSRange::new(offset, 0));
        let (Some(layout), Some(scroll)) = (view.textLayoutManager(), view.enclosingScrollView()) else { return };
        let Some(content) = layout.textContentManager() else { return };
        let Some(fragment) = content.locationFromLocation_withOffset(&content.documentRange().location(), offset as isize).and_then(|at| layout.textLayoutFragmentForLocation(&at)) else { return };
        let y = fragment.layoutFragmentFrame().origin.y + view.textContainerOrigin().y;
        let clip = scroll.contentView();
        clip.scrollToPoint(NSPoint::new(clip.bounds().origin.x, y - scroll.contentInsets().top));
        scroll.reflectScrolledClipView(&clip);
        layout.textViewportLayoutController().layoutViewport();
    }

    /// The offset (UTF-16) of the paragraph at the top of the note's visible part, under the
    /// toolbar.
    pub fn note_top(&self) -> Option<usize> {
        let view = &self.note_text;
        let (layout, scroll) = (view.textLayoutManager()?, view.enclosingScrollView()?);
        let content = layout.textContentManager()?;
        let y = scroll.contentView().bounds().origin.y + scroll.contentInsets().top - view.textContainerOrigin().y + 1.0;
        let fragment = layout.textLayoutFragmentForPosition(NSPoint::new(1.0, y.max(0.0)))?;
        Some(content.offsetFromLocation_toLocation(&content.documentRange().location(), &fragment.rangeInElement().location()).max(0) as usize)
    }

    /// The note column as it should be: the note, the PDF (its viewer covers it), or, with no
    /// document open, a word saying so and no text view to type into (typed text went nowhere).
    /// A recently deleted entry shown: its note read only, its PDF, or words for its folder.
    pub(super) fn show_note_column(&self) {
        let previewing = self.previewing.borrow();
        let pdf = self.pdf_view.borrow().is_some();
        let note = !pdf && previewing.as_ref().map_or(self.editing.borrow().is_some(), |entry| !entry.folder);
        let label = match previewing.as_ref() {
            Some(entry) if entry.folder => strings::deleted_folder_shown(entry.path.rsplit('/').next().unwrap_or(&entry.path), entry.notes),
            _ => strings::NO_DOCUMENT.to_owned(),
        };
        self.no_document.setStringValue(&NSString::from_str(&label));
        self.no_document.setHidden(note || pdf);
        self.note_text.setEditable(previewing.is_none() && (note || pdf));
        if let Some(scroll) = self.note_text.enclosingScrollView() {
            scroll.setHidden(!note);
        }
    }

    /// `empty: …`, `note (editable)` or `pdf`: what the note column shows (probe mode).
    pub fn describe_note_column(&self) -> String {
        if !self.no_document.isHidden() {
            return format!("empty: {}", self.no_document.stringValue());
        }
        if self.pdf_view.borrow().is_some() {
            return "pdf".to_owned();
        }
        format!("note ({})", if self.note_text.isEditable() { "editable" } else { "read only" })
    }

    /// The note column empty: no document open. False when the note open stays: its edits
    /// could not be written and the user chose to stay.
    pub(super) fn show_nothing(&self) -> bool {
        if !self.leave() {
            return false;
        }
        self.previewing.take();
        self.completion.hide();
        if self.find_bar.is_shown() {
            self.close_find();
        }
        *self.editing.borrow_mut() = None;
        self.show_pdf(None);
        self.live_preview.show(&self.note_text, "", "");
        self.embeds.clear();
        self.show_note_column();
        self.refresh_panel();
        self.update_navigation();
        true
    }

    /// The note open stays (its edits could not be written and the user chose to stay): the
    /// list shows it selected again. Gives false, for `open_note` and `open_pdf` to return.
    pub(super) fn stay(&self) -> bool {
        self.highlight_open_note();
        false
    }

    /// Shows a note in the note column; `record` adds it to the back/forward history. False
    /// when the note open stays (its edits could not be written and the user chose to stay).
    pub fn open_note(&self, path: &str, record: bool) -> bool {
        if FileKind::from_path(path) == FileKind::Pdf {
            return self.open_pdf(path, None, record);
        }
        self.remember_place();
        if !self.leave() {
            return self.stay();
        }
        self.previewing.take();
        self.show_pdf(None);
        self.completion.hide();
        let text = {
            let mut state = self.state.borrow_mut();
            if record {
                state.history.visit(path);
            }
            // The file, not the index: the index may not have caught up with the last save yet.
            state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(path).ok()).unwrap_or_default()
        };
        *self.editing.borrow_mut() = Some(Editing { path: path.to_owned(), document: Document::open(&text) });
        self.show_note_column();
        self.live_preview.show(&self.note_text, path, &text);
        self.sync_embeds(true);
        // Where it was when its tab was left; words found by a search, below, win.
        if let Some(place) = self.show_tab(path) {
            self.scroll_note_to(place.top.0);
            if let Some((at, length)) = place.cursor.filter(|(at, length)| at + length <= self.note_text.string().length()) {
                self.note_text.setSelectedRange(NSRange::new(at, length));
            }
        }
        self.show_found_words(&text);
        if self.find_bar.is_shown() {
            self.find_text(&self.find_bar.text(), true);
        }
        self.find_search_word();
        self.refresh_panel();
        self.update_navigation();
        true
    }

    /// Opens the PDF at `path` in place of the note (ADR 0022), at page `page` (from 1). False
    /// when the note open stays (its edits could not be written and the user chose to stay).
    pub fn open_pdf(&self, path: &str, page: Option<u32>, record: bool) -> bool {
        self.remember_place();
        if !self.leave() {
            return self.stay();
        }
        self.previewing.take();
        self.completion.hide();
        if self.find_bar.is_shown() {
            self.close_find();
        }
        if record {
            self.state.borrow_mut().history.visit(path);
        }
        // Opened from the note (its view now hidden): the PDF takes the keys; from the list, the
        // focus stays there.
        let from_note = self.from_note.replace(false) || self.window.firstResponder().is_some_and(|r| std::ptr::eq(&*r as *const _ as *const u8, &*self.note_text as *const _ as *const u8));
        *self.editing.borrow_mut() = None;
        self.embeds.clear();
        self.show_pdf(Some(path));
        self.show_note_column();
        if from_note && let Some(viewer) = self.pdf_view.borrow().as_ref() {
            self.window.makeFirstResponder(Some(viewer.pages_view()));
        }
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            // In its place in the window first, so the jump lands where it stays.
            self.window.layoutIfNeeded();
            viewer.go_to(page.unwrap_or(1).max(1) as usize - 1);
        }
        // Where it was when its tab was left, unless a link names a page.
        let place = self.show_tab(path);
        if let (None, Some(place), Some(viewer)) = (page, place, self.pdf_view.borrow().as_ref()) {
            viewer.show_place(place.top);
        }
        // Opened from a content search: Cmd+F's bar with its word, at its first place (once
        // found, on another thread); a link naming a page keeps it.
        if page.is_none() {
            self.find_search_word();
        }
        self.refresh_panel();
        self.update_navigation();
        true
    }

    /// Shows the PDF at `path` in the note column, or the note again (`None`). The last PDF's
    /// viewer goes, with every page it drew.
    pub(super) fn show_pdf(&self, path: Option<&str>) {
        let file = path.zip(self.vault()).map(|(p, vault)| vault.join(p));
        let shown = self.show_pdf_file(file.as_deref());
        *self.pdf_path.borrow_mut() = path.filter(|_| shown).map(str::to_owned);
    }

    /// Shows the PDF `file` in its viewer in the note column, or none; whether it shows.
    pub(super) fn show_pdf_file(&self, file: Option<&Path>) -> bool {
        self.pdf_view.borrow_mut().take();
        let mtm = objc2::MainThreadOnly::mtm(&*self.window);
        let viewer = file.and_then(|f| PdfViewer::new(mtm, f, self.find_bar.column.bounds()));
        let shown = viewer.is_some();
        if let Some(viewer) = viewer {
            let weak = self.this.borrow().clone();
            viewer.on_scroll(move || {
                if let Some(window) = weak.upgrade() {
                    window.outline_follow();
                }
            });
            // Under the find bar: Cmd+F finds in the PDF too.
            self.find_bar.add_under(&viewer.scroll);
            *self.pdf_view.borrow_mut() = Some(viewer);
        }
        if let Some(scroll) = self.note_text.enclosingScrollView() {
            scroll.setHidden(shown);
        }
        shown
    }

    /// `doc.pdf 2/3` for the PDF shown and its page, or `hidden` (probe mode).
    pub fn describe_pdf(&self) -> String {
        let path = self.pdf_path.borrow().clone().or_else(|| self.previewing.borrow().as_ref().map(|e| e.path.clone()));
        let (Some(path), Some(viewer)) = (path, self.pdf_view.borrow().as_ref().map(|v| v.page())) else { return "hidden".to_owned() };
        format!("{path} {}/{}", viewer.0, viewer.1)
    }

    /// The text selected in the PDF shown (probe mode).
    pub fn pdf_selection(&self) -> String {
        self.pdf_view.borrow().as_ref().map(PdfViewer::selected_text).unwrap_or_default()
    }

    /// The pages whose text the PDF viewer keeps, from 1 (probe mode).
    pub fn pdf_texts_kept(&self) -> Vec<usize> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::texts_kept).unwrap_or_default()
    }

    /// The pages the PDF viewer keeps drawn, from 1 (probe mode).
    pub fn pdf_pages_kept(&self) -> Vec<usize> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::kept).unwrap_or_default()
    }

    /// Shows page `page` (from 1) of the PDF shown (probe mode).
    pub fn pdf_go_to(&self, page: usize) {
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.go_to(page.max(1) - 1);
        }
    }

    /// Image `line`'s edge dragged to `width` and let go (probe mode).
    pub fn resize_image(&self, line: usize, width: f64) {
        self.live_preview.resize_image(&self.note_text, line, width, true);
    }

    /// What each of the note's images shows (probe mode).
    pub fn images_shown(&self) -> Vec<String> {
        self.live_preview.images_shown()
    }

    /// The PDF viewer made `width` wide (probe mode).
    pub fn pdf_set_width(&self, width: f64) {
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            let frame = viewer.scroll.frame();
            viewer.scroll.setFrameSize(objc2_foundation::NSSize::new(width, frame.size.height));
        }
    }

    /// The PDF viewer's view taking the keys (probe mode).
    pub fn pdf_pages_view(&self) -> Option<Retained<NSView>> {
        self.pdf_view.borrow().as_ref().map(|v| objc2::Message::retain(v.pages_view()))
    }

    /// The PDF viewer's page label and whether it shows (probe mode).
    pub fn pdf_page_label(&self) -> Option<String> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::describe_page_label)
    }

    /// How far the PDF viewer is scrolled and its magnification (probe mode).
    pub fn pdf_scroll_and_zoom(&self) -> Option<(f64, f64)> {
        self.pdf_view.borrow().as_ref().map(|v| (v.scroll_y(), v.magnification()))
    }

    /// The page at the top of the PDF viewer, from 1 (probe mode).
    pub fn pdf_top_page(&self) -> Option<usize> {
        self.pdf_view.borrow().as_ref().map(|v| v.top_place().0 + 1)
    }

    /// What each page the PDF viewer keeps shows (probe mode).
    pub fn pdf_drawn(&self) -> Vec<String> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::drawn).unwrap_or_default()
    }

    /// How many of the open note's images have been decoded, and how many are kept (probe
    /// mode).
    pub fn decoded_images(&self) -> (Vec<String>, usize) {
        self.live_preview.decoded_images()
    }

    /// `light` or `dark`: the colour formulas are typeset in (probe mode).
    pub fn formula_ink(&self) -> &'static str {
        self.live_preview.formula_ink()
    }

    /// Probe lines of what the note draws in place of hidden source, in view.
    pub fn describe_drawn(&self) -> Vec<String> {
        LivePreview::describe_drawn(&self.note_text)
    }

    /// Path of the note in the editor.
    pub fn open_path(&self) -> Option<String> {
        self.editing.borrow().as_ref().map(|e| e.path.clone())
    }

    pub fn vault(&self) -> Option<PathBuf> {
        self.state.borrow().vault.clone()
    }

    /// The open note's file as it is on disk now (probe mode).
    pub fn disk_text(&self) -> Option<String> {
        let path = self.open_path()?;
        let vault = self.state.borrow().vault.clone()?;
        FsVault::new(&vault).read_text(&path).ok()
    }

    /// Whether the editor has edits not written yet.
    pub fn is_dirty(&self) -> bool {
        self.editing.borrow().as_ref().is_some_and(|e| e.document.is_dirty())
    }

    /// Opens the current note again (editor bench).
    pub fn reopen_current(&self) {
        let current = self.state.borrow().history.current().map(str::to_owned);
        if let Some(path) = current {
            self.open_note(&path, false);
        }
    }

    pub fn live_preview_restyle(&self, source: &str) {
        self.live_preview.restyle_all(&self.note_text, source);
    }

    /// Back (`false`) or forward (`true`) in the note history; the list selection follows.
    pub fn navigate(&self, forward: bool) {
        let target = {
            let mut state = self.state.borrow_mut();
            let next = if forward { state.history.forward() } else { state.history.back() };
            next.map(str::to_owned)
        };
        if let Some(path) = target {
            if !self.open_note(&path, false) {
                // The note open stayed: the history goes back to it.
                let mut state = self.state.borrow_mut();
                if forward { state.history.back() } else { state.history.forward() };
                drop(state);
                self.update_navigation();
                return;
            }
            self.highlight_open_note();
        }
    }

    pub(super) fn update_navigation(&self) {
        let state = self.state.borrow();
        self.toolbar_delegate.set_history(state.history.can_back(), state.history.can_forward());
        drop(state);
        // A note's name; a PDF's, without its folder or `.pdf` (owner's choice). Both are renamed
        // in place.
        match (self.open_path(), self.pdf_path.borrow().as_deref()) {
            (Some(note), _) => self.toolbar_delegate.set_note_title(knotes_app::title_of(&note), true),
            (None, Some(pdf)) => {
                let name = pdf.rsplit('/').next().unwrap_or(pdf);
                let stem = if name.to_lowercase().ends_with(".pdf") { &name[..name.len() - 4] } else { name };
                self.toolbar_delegate.set_note_title(stem, true);
            }
            // A recently deleted entry shown: its name, not to be renamed.
            (None, None) => {
                let name = self.previewing.borrow().as_ref().map(|e| {
                    let name = e.path.rsplit('/').next().unwrap_or(&e.path);
                    if e.folder { name.to_owned() } else { knotes_app::title_of(name).to_owned() }
                });
                self.toolbar_delegate.set_note_title(name.as_deref().unwrap_or(""), false);
            }
        }
    }

    /// The open note and history availability, for probe mode.
    pub fn describe_note(&self) -> (Option<String>, bool, bool) {
        let state = self.state.borrow();
        (state.history.current().map(str::to_owned), state.history.can_back(), state.history.can_forward())
    }

    pub(super) fn ensure_reader(&self, state: &mut State) {
        if state.reader.is_none() {
            state.reader = state.index_dir.as_deref().and_then(|dir| SqliteIndex::open_reader(dir).ok());
        }
    }
}
