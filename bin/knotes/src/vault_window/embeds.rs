//! The notes the open note embeds (ADR 0034): shown, edited in place and written back, opened.

use super::*;

impl VaultWindow {
    /// Follows a clicked link: opens the note it resolves to (creating a missing one), then
    /// moves to its `#heading` or `^block`; external links open in their app (ADR 0013).
    /// The notes the open note embeds alone on their lines, in boxes (ADR 0034); their notes read
    /// again when `reload`, else only embeds new since the last time.
    pub(super) fn sync_embeds(&self, reload: bool) {
        let (Some(source), Some(vault)) = (self.open_path(), self.vault()) else {
            self.embeds.clear();
            return;
        };
        let links = self.state.borrow().links.clone();
        let (link_weak, edit_weak, focus_weak) = (self.this.borrow().clone(), self.this.borrow().clone(), self.this.borrow().clone());
        let open_weak = self.this.borrow().clone();
        let callbacks = Callbacks {
            on_link: Rc::new(move |link, from| {
                if let Some(window) = link_weak.upgrade() {
                    window.follow_link_from(link, Some(from));
                }
            }),
            on_edit: Rc::new(move |key| {
                if let Some(window) = edit_weak.upgrade() {
                    window.embeds.edited(key, &window.note_text, &window.live_preview);
                    window.save_embeds_later();
                    window.complete_in_embed(key);
                    // An edit drops the highlights: the occurrences are found again, in place.
                    if window.find_bar.is_shown() {
                        window.find_text(&window.find_bar.text(), false);
                    }
                }
            }),
            on_cursor: Rc::new({
                let weak = self.this.borrow().clone();
                move |key| {
                    if let Some(window) = weak.upgrade() {
                        window.complete_in_embed(key);
                    }
                }
            }),
            on_command: Rc::new({
                let weak = self.this.borrow().clone();
                move |command| weak.upgrade().is_some_and(|window| window.completion_command(command))
            }),
            on_focus: Rc::new(move |key, taken| {
                if let Some(window) = focus_weak.upgrade() {
                    window.embeds.refocus(key);
                    if !taken {
                        window.save_embeds();
                    }
                }
            }),
            // After the button's action: opening the note takes the button away.
            on_open: Rc::new(move |key| {
                let (Some(mtm), key) = (MainThreadMarker::new(), key.to_owned()) else { return };
                let target = MainThreadBound::new(open_weak.clone(), mtm);
                DispatchQueue::main().exec_async(move || {
                    let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                    if let Some(window) = target.get(mtm).upgrade() {
                        window.open_embed(&key);
                    }
                });
            }),
        };
        let resolver = links.clone().map(|l| l as Arc<dyn LinkResolver>);
        let files = FsVault::new(&vault);
        self.embeds.sync(&self.note_text, &self.live_preview, resolver, &callbacks, reload, |link| {
            let path = links.as_ref()?.resolve(link.kind, &link.target, &source)?;
            // A note embedding itself would hold itself without end.
            if FileKind::from_path(&path) != FileKind::Markdown || path == source {
                return None;
            }
            // A note not in UTF-8 stays a link: edited here, it would lose its bytes.
            let full = files.read_utf8(&path).ok()?;
            let range = embedded_part(&full, link.subpath.as_deref())?;
            Some(Content { path, full, range, subpath: link.subpath.clone() })
        });
    }

    /// Opens the note the embed `key` shows, at its section or block (its edits written first,
    /// as the note is left).
    pub(super) fn open_embed(&self, key: &str) {
        let Some((path, subpath)) = self.embeds.target_of(key) else { return };
        if !self.open_note(&path, true) {
            return;
        }
        self.highlight_open_note();
        if let Some(subpath) = subpath {
            self.show_subpath(&subpath);
        }
    }

    /// The `n`th embedded note's view (probe mode).
    pub fn embed_view(&self, n: usize) -> Option<Retained<NSTextView>> {
        self.embeds.view(n).map(Retained::into_super)
    }

    /// A click on character `index` of the `n`th embedded note (probe mode).
    pub fn probe_click_in_embed(&self, n: usize, index: usize) {
        self.embeds.probe_click(n, index);
    }

    /// Opens the note of the `n`th embed, as its button does (probe mode).
    pub fn probe_open_embed(&self, n: usize) {
        if let Some(key) = self.embeds.key(n) {
            self.open_embed(&key);
        }
    }

    /// The completion list for the cursor in the embed `key`'s view, links written as from its
    /// note.
    pub(super) fn complete_in_embed(&self, key: &str) {
        if let Some((view, path)) = self.embeds.view_of(key) {
            self.complete_in(&view, Some(path));
        }
    }

    /// Writes what was typed in embedded notes back into them (ADR 0034); a failure is told.
    pub fn save_embeds(&self) {
        if let Err((path, e)) = self.write_embeds() {
            self.tell_save_failed(&path, &e);
        }
    }

    /// Writes what was typed in embedded notes back into them; the first that failed.
    pub(super) fn write_embeds(&self) -> Result<(), SaveFailure> {
        match self.vault() {
            Some(vault) => self.embeds.save(&FsVault::new(&vault)),
            None => Ok(()),
        }
    }

    /// An edit in an embedded note: written once the user pauses, as the note's own.
    pub(super) fn save_embeds_later(&self) {
        let edit = self.embed_edits.get() + 1;
        self.embed_edits.set(edit);
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let when = DispatchTime::try_from(AUTOSAVE_DELAY).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(window) = target.get(mtm).upgrade()
                && window.embed_edits.get() == edit
            {
                window.save_embeds();
            }
        });
    }

    /// Types `text` at the end of the `n`th embedded note, as the user would there (probe mode).
    pub fn probe_type_in_embed(&self, n: usize, text: &str) {
        let Some(view) = self.embeds.view(n) else { return };
        self.window.makeFirstResponder(Some(&view));
        view.setSelectedRange(NSRange::new(view.string().length(), 0));
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { NSTextInputClient::insertText_replacementRange(&**view, &NSString::from_str(text), not_found) };
    }

    /// The embedded notes' views, for the probe.
    pub fn describe_embeds(&self) -> Vec<String> {
        self.embeds.describe()
    }
}
