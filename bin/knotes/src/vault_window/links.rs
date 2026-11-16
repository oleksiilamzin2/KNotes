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
        if link.kind == LinkKind::External {
            open_url(&link.target);
            return;
        }
        let Some(open) = self.open_path() else { return };
        let source = from.map_or_else(|| open.clone(), str::to_owned);
        let Some(links) = self.state.borrow().links.clone() else { return };
        let path = match links.resolve(link.kind, &link.target, &source) {
            Some(path) => path,
            None => match self.create_linked_note(link, &source) {
                Some(path) => path,
                None => return,
            },
        };
        if FileKind::from_path(&path) == FileKind::Pdf {
            let page = link.subpath.as_deref().and_then(|s| s.strip_prefix("page=")).and_then(|p| p.parse().ok());
            // From the note: the PDF takes the keys.
            self.from_note.set(true);
            self.open_pdf(&path, page, true);
            self.from_note.set(false);
            return;
        }
        if path != open {
            self.open_note(&path, true);
            self.highlight_open_note();
        }
        if let Some(subpath) = &link.subpath {
            self.show_subpath(subpath);
        }
    }

    /// A link in a chat answer (`Projects/Radar`, `budget.pdf#page=3`): the note or PDF it
    /// resolves to opened, at its heading or page. A link to nothing opens nothing (unlike a
    /// note's links, which make the note).
    pub(super) fn open_chat_link(&self, target: &str) {
        let (path, subpath) = match target.split_once('#') {
            Some((path, subpath)) => (path, Some(subpath.to_owned())),
            None => (target, None),
        };
        let Some(links) = self.state.borrow().links.clone() else { return };
        let source = self.open_path().unwrap_or_default();
        let Some(resolved) = links.resolve(LinkKind::Wiki, path, &source) else { return };
        if FileKind::from_path(&resolved) == FileKind::Pdf {
            let page = subpath.as_deref().and_then(|s| s.strip_prefix("page=")).and_then(|p| p.parse().ok());
            self.open_pdf(&resolved, page, true);
            return;
        }
        self.open_note(&resolved, true);
        self.highlight_open_note();
        if let Some(subpath) = &subpath {
            let text = self.note_text.string().to_string();
            if let Some(at) = subpath_offset(&text, subpath) {
                self.note_text.move_to_top(utf16_offsets(&text, &[at])[0]);
            }
        }
    }

    /// Creates the empty note a link to a missing note names: at the written path for links
    /// with a folder (Markdown links: from the linking note's folder), else in the new notes
    /// folder. An existing file (not indexed yet) is opened as it is.
    pub(super) fn create_linked_note(&self, link: &Link, source: &str) -> Option<String> {
        let target = link.target.trim().trim_start_matches('/');
        if target.is_empty() {
            return None;
        }
        let state = self.state.borrow();
        let vault = state.vault.clone()?;
        let folder = match link.kind {
            LinkKind::Markdown => source.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_owned()),
            _ if target.contains('/') => String::new(),
            _ => new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
        };
        drop(state);
        let mut path = normalize(&if folder.is_empty() { target.to_owned() } else { format!("{folder}/{target}") })?;
        if !path.to_lowercase().ends_with(".md") {
            path.push_str(".md");
        }
        let fs = FsVault::new(&vault);
        if !vault.join(&path).exists()
            && let Err(e) = fs.write_note(&path, "")
        {
            eprintln!("knotes: cannot create {path}: {e}");
            return None;
        }
        Some(path)
    }
}
