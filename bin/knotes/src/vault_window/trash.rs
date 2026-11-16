//! The recently deleted (ADR 0032): deleting, restoring, deleting for good, and an entry shown where it is kept.

use super::*;

/// Now, in Unix milliseconds.
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

impl VaultWindow {
    /// Moves a note, a PDF or a folder into the recently deleted (ADR 0032); what is open of
    /// it closes first (its edits written).
    pub fn delete(&self, path: &str) {
        let Some(vault) = self.vault() else { return };
        // Edits typed in an embed of it go with it, not beside it after it went.
        self.save_embeds();
        let inside = format!("{path}/");
        let open: Vec<String> = self.tabs.borrow().paths().filter(|p| *p == path || p.starts_with(&inside)).map(str::to_owned).collect();
        for tab in open {
            // Its edits could not be written and the user stayed with them: nothing is deleted.
            if !self.close_tab(&tab) {
                return;
            }
        }
        if let Err(e) = FsVault::new(&vault).trash(path, now_ms()) {
            self.tell_failed(&strings::delete_failed(document_name(path)), &e);
            return;
        }
        self.refresh_trash();
    }

    /// A folder's menu: into the recently deleted, once confirmed when it holds notes.
    pub fn delete_folder(&self, folder: &str) {
        let notes = {
            let state = self.state.borrow();
            find_folder_node(&state.folders, folder).map_or(0, |n| n.notes)
        };
        if notes > 0 && !probe::enabled() {
            let days = trash_days(&self.state.borrow().rules);
            let alert = NSAlert::new(MainThreadMarker::from(&*self.window));
            alert.setMessageText(&NSString::from_str(&strings::delete_folder_question(folder.rsplit('/').next().unwrap_or(folder), notes)));
            alert.setInformativeText(&NSString::from_str(&strings::delete_kept_note(days)));
            for title in [strings::DELETE, strings::CANCEL] {
                alert.addButtonWithTitle(&NSString::from_str(title));
            }
            if alert.runModal() != objc2_app_kit::NSAlertFirstButtonReturn {
                return;
            }
        }
        self.delete(folder);
    }

    /// Brings a deleted entry back where it was (ADR 0032).
    pub fn restore(&self, id: &str) {
        let Some(vault) = self.vault() else { return };
        let entry = self.state.borrow().trash.iter().find(|entry| entry.id == id).cloned();
        match FsVault::new(&vault).restore(id) {
            // Back under another name (its own taken): a setting of its own for the AI, which
            // stayed with its old name while it was deleted, says the same of the new (ADR 0036).
            Ok(restored) => {
                if let Some(entry) = entry.filter(|entry| !entry.folder && entry.path != restored) {
                    self.carry_ai([(entry.path.as_str(), restored.as_str())], true);
                }
            }
            Err(e) => {
                let name = entry.map(|entry| document_name(&entry.path).to_owned()).unwrap_or_default();
                self.tell_failed(&strings::restore_failed(&name), &e);
            }
        }
        self.refresh_trash();
    }

    /// Deletes an entry for good, or all of them (once confirmed).
    pub fn purge(&self, id: Option<&str>) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        let ids: Vec<String> = match id {
            Some(id) => vec![id.to_owned()],
            None => {
                let all: Vec<String> = self.state.borrow().trash.iter().map(|e| e.id.clone()).collect();
                if all.is_empty() {
                    return;
                }
                if !probe::enabled() {
                    let alert = NSAlert::new(MainThreadMarker::from(&*self.window));
                    alert.setMessageText(&NSString::from_str(&strings::empty_deleted_question(all.len())));
                    for title in [strings::DELETE_FOREVER, strings::CANCEL] {
                        alert.addButtonWithTitle(&NSString::from_str(title));
                    }
                    if alert.runModal() != objc2_app_kit::NSAlertFirstButtonReturn {
                        return;
                    }
                }
                all
            }
        };
        let failed: Vec<(String, std::io::Error)> = ids.iter().filter_map(|id| files.purge(id).err().map(|e| (id.clone(), e))).collect();
        if let Some((_, e)) = failed.first() {
            let trash = Arc::clone(&self.state.borrow().trash);
            let names: Vec<&str> = failed.iter().filter_map(|(id, _)| trash.iter().find(|entry| &entry.id == id).map(|entry| document_name(&entry.path))).collect();
            self.tell_failed(strings::PURGE_FAILED, &strings::failed_files(&e.to_string(), &names));
        }
        self.refresh_trash();
    }

    /// Whether the recently deleted entry `id` is the one shown.
    pub(super) fn shows_deleted(&self, id: &str) -> bool {
        self.previewing.borrow().as_ref().is_some_and(|e| e.id == id)
    }

    /// A recently deleted entry shown where it is kept, to be read: a note's text read only, a
    /// PDF in its viewer, a folder in words. No tab, no history, nothing written (an edit
    /// would bring a note back where it was).
    pub fn show_deleted(&self, id: &str) {
        let Some(entry) = self.state.borrow().trash.iter().find(|e| e.id == id).cloned() else { return };
        let Some(kept) = self.vault().and_then(|v| FsVault::new(&v).kept_at(id).ok()) else { return };
        if !self.leave() {
            self.stay();
            return;
        }
        self.completion.hide();
        if self.find_bar.is_shown() {
            self.close_find();
        }
        *self.editing.borrow_mut() = None;
        self.show_pdf(None);
        let pdf = !entry.folder && FileKind::from_path(&entry.path) == FileKind::Pdf;
        let text = if entry.folder || pdf { String::new() } else { std::fs::read_to_string(&kept).unwrap_or_default() };
        if pdf {
            self.show_pdf_file(Some(&kept));
        }
        self.live_preview.show(&self.note_text, &entry.path, &text);
        self.embeds.clear();
        *self.previewing.borrow_mut() = Some(entry);
        self.show_note_column();
        self.scroll_note_to(0);
        self.refresh_panel();
        self.update_navigation();
    }

    /// The recently deleted read again (the watcher does not see them), the sidebar's row and
    /// their list shown again.
    pub(super) fn refresh_trash(&self) {
        let Some(vault) = self.vault() else { return };
        let trash = FsVault::new(&vault).trashed().unwrap_or_default();
        self.state.borrow_mut().trash = Arc::new(trash);
        self.rebuild_sidebar();
        let mut selection = self.state.borrow().selection.clone();
        if !self.exists(&selection) {
            selection = SidebarItem::AllNotes;
            self.state.borrow_mut().selection = selection.clone();
        }
        self.highlight(&selection);
        self.refresh_list();
    }
}
