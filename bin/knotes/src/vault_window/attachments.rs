//! The attachments' window (File → Attachments…): what it lists, read from the index, and
//! what it asks done.

use super::*;
use crate::attachments_window::{Asks, AttachmentsWindow};

impl VaultWindow {
    /// The vault's attachments and its links to missing ones, in a window of their own.
    pub fn show_attachments(&self) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        if self.attachments.borrow().is_none() {
            let this = || self.this.borrow().clone();
            let (deletes, reveals, opens, files) = (this(), this(), this(), this());
            let window = AttachmentsWindow::new(
                mtm,
                Asks {
                    delete: Box::new(move |paths, linked| deletes.upgrade().is_some_and(|window| window.delete_attachments(paths, linked))),
                    reveal: Box::new(move |paths| {
                        let Some(vault) = reveals.upgrade().and_then(|window| window.vault()) else { return };
                        let urls: Vec<_> = paths.iter().map(|path| objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(&vault.join(path).to_string_lossy()))).collect();
                        objc2_app_kit::NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&objc2_foundation::NSArray::from_retained_slice(&urls));
                    }),
                    file: Box::new(move |path| files.upgrade().and_then(|window| window.vault()).map(|vault| vault.join(path))),
                    open: Box::new(move |path| {
                        if let Some(window) = opens.upgrade() {
                            window.open_attachment(path);
                        }
                    }),
                },
            );
            *self.attachments.borrow_mut() = Some(window);
        }
        let (files, missing) = self.read_attachments();
        if let Some(window) = self.attachments.borrow().as_ref() {
            window.show(files, missing);
        }
    }

    fn read_attachments(&self) -> (Vec<knotes_core::Attachment>, Vec<knotes_core::MissingAttachment>) {
        let mut state = self.state.borrow_mut();
        self.ensure_reader(&mut state);
        state.attachments()
    }

    /// The vault changed: the attachments' window, while it shows, lists what is there now.
    pub(super) fn reload_attachments(&self) {
        if self.attachments.borrow().as_ref().is_some_and(|window| window.is_shown()) {
            let (files, missing) = self.read_attachments();
            if let Some(window) = self.attachments.borrow().as_ref() {
                window.set(files, missing);
            }
        }
    }

    /// The attachments at `paths` into the recently deleted, once confirmed (`linked` of them
    /// have links to them); whether they went.
    fn delete_attachments(&self, paths: &[String], linked: usize) -> bool {
        let Some(vault) = self.vault() else { return false };
        // With links to them, the question offers to take those out of the notes too.
        let with_links = if probe::enabled() {
            std::env::var_os("KNOTES_PROBE_ATTACHMENTS_LINKS").is_some()
        } else {
            let days = trash_days(&self.state.borrow().rules);
            let alert = NSAlert::new(MainThreadMarker::from(&*self.window));
            alert.setMessageText(&NSString::from_str(&strings::attachments_delete_question(paths.len(), document_name(&paths[0]))));
            alert.setInformativeText(&NSString::from_str(&strings::attachments_delete_details(linked, days)));
            let titles: &[&str] = if linked > 0 { &[strings::ATTACHMENTS_DELETE_WITH_LINKS, strings::ATTACHMENTS_DELETE_FILES, strings::CANCEL] } else { &[strings::DELETE, strings::CANCEL] };
            for title in titles {
                alert.addButtonWithTitle(&NSString::from_str(title));
            }
            let answer = alert.runModal();
            if answer - objc2_app_kit::NSAlertFirstButtonReturn >= titles.len() as isize - 1 {
                return false;
            }
            linked > 0 && answer == objc2_app_kit::NSAlertFirstButtonReturn
        };
        if with_links && !self.unlink(paths) {
            return false;
        }
        let files = FsVault::new(&vault);
        // One shown in the note column closes first, as a note deleted does.
        for path in paths {
            for tab in self.tabs.borrow().under(path) {
                self.close_tab(&tab);
            }
        }
        let failed: Vec<(&str, std::io::Error)> = paths.iter().filter_map(|path| files.trash(path, trash::now_ms()).err().map(|e| (document_name(path), e))).collect();
        if let Some((_, e)) = failed.first() {
            let names: Vec<&str> = failed.iter().map(|(name, _)| *name).collect();
            self.tell_failed(strings::ATTACHMENTS_DELETE_FAILED, &strings::failed_files(&e.to_string(), &names));
        }
        self.refresh_trash();
        failed.len() < paths.len()
    }

    /// Every link to the files at `paths` taken out of the notes' text (what is typed is
    /// written first: the notes' files are read). False when it could not be done: nothing is
    /// deleted then.
    fn unlink(&self, paths: &[String]) -> bool {
        let _ = self.write_all();
        let fresh: Vec<String> = self.open_path().into_iter().chain(self.embeds.paths()).collect();
        let plan = {
            let mut state = self.state.borrow_mut();
            self.ensure_reader(&mut state);
            let (Some(reader), Some(links), Some(vault)) = (state.reader.as_ref(), state.links.clone(), state.vault.clone()) else { return false };
            let files = FsVault::new(&vault);
            // A note not in UTF-8 is left as it is: written back, it would lose its bytes.
            knotes_app::plan_unlinks(paths, reader, &|p| files.read_utf8(p).ok(), &MarkdownParser::new(), &*links, &fresh)
        };
        let plan = match plan {
            Ok(plan) => plan,
            Err(e) => {
                self.tell_failed(strings::LINKS_FAILED, &e);
                return false;
            }
        };
        let Some(vault) = self.vault() else { return false };
        let files = FsVault::new(&vault);
        let failed: Vec<(&str, std::io::Error)> = plan.edits.iter().filter_map(|(path, text)| files.write_note(path, text).err().map(|e| (path.as_str(), e))).collect();
        if let Some((_, e)) = failed.first() {
            let names: Vec<&str> = failed.iter().map(|(path, _)| document_name(path)).collect();
            self.tell_failed(strings::LINKS_FAILED, &strings::failed_files(&e.to_string(), &names));
        }
        // The note open, when it was one of them: shown as it is written now.
        if let Some(open) = self.open_path()
            && let Some((_, text)) = plan.edits.iter().find(|(path, _)| *path == open).filter(|(path, _)| !failed.iter().any(|(f, _)| f == path))
        {
            if let Some(editing) = self.editing.borrow_mut().as_mut() {
                editing.document.saved(text);
            }
            let cursor = self.note_text.selectedRange();
            self.live_preview.show(&self.note_text, &open, text);
            let end = self.note_text.string().length();
            self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
        }
        true
    }

    /// A double click in the attachments' window: a note or a PDF opened here, another file
    /// in the app the Mac opens it with.
    fn open_attachment(&self, path: &str) {
        match FileKind::from_path(path) {
            FileKind::Markdown => {
                self.open_note(path, true);
                self.window.makeKeyAndOrderFront(None);
            }
            FileKind::Pdf => {
                self.open_pdf(path, None, true);
                self.window.makeKeyAndOrderFront(None);
            }
            _ => {
                if let Some(vault) = self.vault() {
                    let url = objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(&vault.join(path).to_string_lossy()));
                    objc2_app_kit::NSWorkspace::sharedWorkspace().openURL(&url);
                }
            }
        }
    }

    /// The attachments' window, once shown (probe mode).
    pub fn attachments_window(&self) -> std::cell::Ref<'_, Option<AttachmentsWindow>> {
        self.attachments.borrow()
    }
}
