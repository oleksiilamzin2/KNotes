//! Links followed: from the note, an embedded note or the chat, to a note (made when missing), a PDF or the web.

use super::*;

/// Opens an external link in its app; probe mode only reports it.
pub(super) fn open_url(url: &str) {
    if probe::enabled() {
        println!("open-url: {url}");
        return;
    }
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

impl VaultWindow {
    /// Puts the cursor at the open note's heading or block `subpath`, scrolled to the top.
    pub(super) fn show_subpath(&self, subpath: &str) {
        let text = self.note_text.string().to_string();
        if let Some(at) = subpath_offset(&text, subpath) {
            let at = utf16_offsets(&text, &[at])[0];
            self.note_text.setSelectedRange(NSRange::new(at, 0));
            // From the layout, not the screen: the window may not be on it yet.
            self.scroll_note_to(at);
        }
    }

    pub(super) fn follow_link(&self, link: &Link) {
        self.follow_link_from(link, None);
    }

    /// Follows `link` as written in the note at `from` (an embedded note, ADR 0034), or in the
    /// open note: it resolves from there, and a missing note is made from there.
    pub(super) fn follow_link_from(&self, link: &Link, from: Option<&str>) {
        let open = self.open_path();
        let source = from.map(str::to_owned).or_else(|| open.clone()).unwrap_or_default();
        let followed = match self.followed(link, &source) {
            // A link of the note open: the note it names is made.
            Followed::Missing if open.is_some() => match self.create_linked_note(link, &source) {
                Some(path) => Followed::Note { path, subpath: link.subpath.clone() },
                None => return,
            },
            Followed::Web(address) => Followed::Web(address),
            // Nothing is open: only the web's address is followed.
            _ if open.is_none() => return,
            followed => followed,
        };
        // From the note: a PDF takes the keys.
        self.from_note.set(matches!(followed, Followed::Pdf { .. }));
        let shown = self.open_followed(followed, open.as_deref());
        self.from_note.set(false);
        if let Some(Some(subpath)) = shown {
            self.show_subpath(&subpath);
        }
    }

    /// Where `link`, written in the note `from`, leads (knotes-app's `follow_link`).
    fn followed(&self, link: &Link, from: &str) -> Followed {
        let links = self.state.borrow().links.clone();
        follow_link(link, from, links.as_deref())
    }

    /// Opens what a link leads to: the web's address in its app, a PDF at its page, a note
    /// (unless it is `open` already). Says the place in a note shown to go to (`Some(None)`:
    /// the note, at no place); none when no note shows for it.
    fn open_followed(&self, followed: Followed, open: Option<&str>) -> Option<Option<String>> {
        match followed {
            Followed::Web(address) => open_url(&address),
            Followed::Pdf { path, page } => {
                self.open_pdf(&path, page, true);
            }
            Followed::Note { path, subpath } => {
                if open != Some(path.as_str()) {
                    if !self.open_note(&path, true) {
                        return None;
                    }
                    self.highlight_open_note();
                }
                return Some(subpath);
            }
            Followed::Missing | Followed::Nothing => {}
        }
        None
    }

    /// A link clicked in a document the vault's search shows, written in the note at `from`:
    /// the note or PDF it resolves to from there, at its heading or page, or the web's address
    /// in the browser. A link to nothing opens nothing (in a note open here it would make the
    /// note; from a search, that would surprise).
    pub(super) fn follow_found_link(&self, link: &Link, from: &str) {
        if let Some(Some(subpath)) = self.open_followed(self.followed(link, from), None) {
            self.show_subpath(&subpath);
        }
    }

    /// A link in a chat answer (`Projects/Radar`, `budget.pdf#page=3`): the note or PDF it
    /// resolves to opened, at its heading or page. A link to nothing opens nothing (unlike a
    /// note's links, which make the note).
    pub(super) fn open_chat_link(&self, target: &str) {
        let source = self.open_path().unwrap_or_default();
        if let Some(Some(subpath)) = self.open_followed(self.followed(&named_link(target), &source), None) {
            let text = self.note_text.string().to_string();
            if let Some(at) = subpath_offset(&text, &subpath) {
                self.note_text.move_to_top(utf16_offsets(&text, &[at])[0]);
            }
        }
    }

    /// Creates the empty note a link to a missing note names (where: knotes-app's
    /// `linked_note_path`). An existing file (not indexed yet) is opened as it is.
    pub(super) fn create_linked_note(&self, link: &Link, source: &str) -> Option<String> {
        let (vault, inbox) = {
            let state = self.state.borrow();
            (state.vault.clone()?, new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER))
        };
        let path = linked_note_path(link, source, &inbox)?;
        if !vault.join(&path).exists()
            && let Err(e) = FsVault::new(&vault).write_note(&path, "")
        {
            self.tell_failed(&strings::create_failed(document_name(&path)), &e);
            return None;
        }
        Some(path)
    }
}
