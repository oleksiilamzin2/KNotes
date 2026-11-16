//! Writing notes: autosave, new notes, renames and moves with their links, conflicts with the disk, pasted images.

use super::*;

/// `path` with `.` and `..` segments applied; `None` if it leaves the vault.
pub(super) fn normalize(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

pub(super) fn write(vault: &FsVault, editing: &mut Editing, text: &str) -> std::io::Result<()> {
    vault.write_note(&editing.path, text)?;
    editing.document.saved(text);
    Ok(())
}

/// What could not be written: the note's path and why.
pub(super) type SaveFailure = (String, std::io::Error);

/// A note as the editor takes it (`read_for_editor`).
pub(super) enum NoteRead {
    /// Its text, to edit.
    Text(String),
    /// Not UTF-8: its text as well as it reads, read only (written, it would lose its bytes).
    NotUtf8(String),
    /// Not readable (offline in iCloud, not allowed, gone): nothing to show, and read only, or
    /// what is typed would replace the file.
    Failed(std::io::Error),
}

pub(super) fn read_for_editor(vault: &FsVault, path: &str) -> NoteRead {
    match vault.read_utf8(path) {
        Ok(text) => NoteRead::Text(text),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => match vault.read_text(path) {
            Ok(text) => NoteRead::NotUtf8(text),
            Err(e) => NoteRead::Failed(e),
        },
        Err(e) => NoteRead::Failed(e),
    }
}

/// An image found on a pasteboard: a file already in the vault (its vault path), or one to
/// add to the attachments folder.
pub(super) enum PastedImage {
    InVault(String),
    New { name: String, bytes: Vec<u8> },
}

/// The images a paste or a drop brings: image and PDF files (others are left), or else an image's
/// data alone; none when text comes with it (a cell copied from a table carries its picture
/// too, and the text is what is meant).
pub(super) fn pasteboard_images(pasteboard: &NSPasteboard, vault: &Path) -> Vec<PastedImage> {
    // SAFETY: AppKit's pasteboard type constants live for the whole process.
    let (file_url, string, png, tiff) = unsafe { (NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardTypePNG, NSPasteboardTypeTIFF) };
    let files: Vec<PathBuf> = pasteboard
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.stringForType(file_url)?;
                    NSURL::URLWithString(&url)?.path().map(|p| PathBuf::from(p.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    if !files.is_empty() {
        let vaults = [vault.to_path_buf(), vault.canonicalize().unwrap_or_default()];
        return files
            .into_iter()
            .filter(|f| f.to_str().is_some_and(|f| matches!(FileKind::from_path(f), FileKind::Image | FileKind::Pdf)))
            .filter_map(|file| {
                let inside = vaults.iter().find_map(|v| file.strip_prefix(v).ok().and_then(Path::to_str).map(str::to_owned));
                match inside {
                    Some(path) => Some(PastedImage::InVault(path)),
                    None => Some(PastedImage::New { name: file.file_name()?.to_str()?.to_owned(), bytes: std::fs::read(&file).ok()? }),
                }
            })
            .collect();
    }
    if pasteboard.stringForType(string).is_some() {
        return Vec::new();
    }
    let bytes = pasteboard.dataForType(png).map(|d| d.to_vec()).or_else(|| {
        let data = pasteboard.dataForType(tiff)?;
        let rep = NSBitmapImageRep::imageRepWithData(&data)?;
        // SAFETY: an empty properties dictionary is valid for PNG.
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }.map(|d| d.to_vec())
    });
    let Some(bytes) = bytes else { return Vec::new() };
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyyMMddHHmmss")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    vec![PastedImage::New { name: pasted_image_name(&stamp), bytes }]
}

/// Writes the disk version of `path` as "Name (конфлікт date time).md" next to it; its path.
pub(super) fn keep_conflict_copy(vault: &FsVault, path: &str, disk: &str) -> std::io::Result<String> {
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    let mut copy = conflict_path(path, &stamp);
    if vault.read_text(&copy).is_ok() {
        formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm.ss")));
        copy = conflict_path(path, &formatter.stringFromDate(&NSDate::now()).to_string());
    }
    vault.write_note(&copy, disk)?;
    Ok(copy)
}

impl VaultWindow {
    /// Images pasted (Cmd+V) or dropped (at character `at`) into the open note: copied into
    /// the attachments folder (one already in the vault stays where it is) and embedded where
    /// the cursor is or the drop falls, each on a line of its own (ADR 0020). False when the
    /// pasteboard brings no image: the text view pastes as usual.
    pub fn paste_images(&self, pasteboard: &NSPasteboard, at: Option<usize>) -> bool {
        let Some(vault) = self.state.borrow().vault.clone() else { return false };
        let found = pasteboard_images(pasteboard, &vault);
        if found.is_empty() {
            return false;
        }
        let store = FsVault::new(&vault);
        let mut failed = Vec::new();
        let paths: Vec<String> = found
            .into_iter()
            .filter_map(|image| match image {
                PastedImage::InVault(path) => Some(path),
                PastedImage::New { name, bytes } => store.add_attachment(&name, &bytes).inspect_err(|e| failed.push((name.clone(), e.to_string()))).ok(),
            })
            .collect();
        if let Some((name, error)) = failed.first() {
            let names: Vec<&str> = failed.iter().map(|(n, _)| n.as_str()).collect();
            self.tell_failed(&strings::attach_failed(name), &strings::failed_files(error, &names[1..]));
        }
        if paths.is_empty() {
            return true;
        }
        let view: &objc2_app_kit::NSTextView = &self.note_text;
        let text = view.string();
        if let Some(at) = at {
            view.setSelectedRange(NSRange::new(at.min(text.length()), 0));
        }
        let selection = view.selectedRange();
        let end = selection.location + selection.length;
        let line_start = text.lineRangeForRange(NSRange::new(selection.location, 0)).location;
        let after_line = text.lineRangeForRange(NSRange::new(end, 0));
        let line_end = (after_line.location + after_line.length).max(end);
        let before = text.substringWithRange(NSRange::new(line_start, selection.location - line_start)).to_string();
        let after = text.substringWithRange(NSRange::new(end, line_end - end)).to_string();
        let catalog = Arc::clone(&self.state.borrow().catalog);
        let bare = |path: &str| {
            let name = path.rsplit('/').next().unwrap_or(path);
            !catalog.iter().any(|e| e.path != path && e.path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        };
        let embeds = embed_images(&paths, &before, after.trim_end_matches(['\n', '\r']), bare);
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&embeds), not_found) };
        true
    }

    /// A new folder in `parent` (none: the folder selected in the sidebar, else the vault's
    /// top), "Нова тека" numbered if taken, its name then edited in its row. Made at the top of
    /// a space, it joins the space (ADR 0030).
    pub fn new_folder(&self, parent: Option<String>) {
        let Some(vault) = self.vault() else { return };
        let parent = parent.unwrap_or_else(|| match &self.state.borrow().selection {
            SidebarItem::Folder(path) => path.clone(),
            _ => String::new(),
        });
        let path = unique_folder_path(&parent, strings::NEW_FOLDER, |p| vault.join(p).exists());
        if let Err(e) = std::fs::create_dir_all(vault.join(&path)) {
            self.tell_failed(strings::NEW_FOLDER_FAILED, &e);
            return;
        }
        let space = self.state.borrow().space.clone().filter(|_| parent.is_empty());
        if let Some(id) = &space {
            let text = set_folder_space("", FolderSpace::In(id)).unwrap_or_default();
            if let Err(e) = FsVault::new(&vault).write_rules(&path, &text) {
                self.tell_failed(&strings::folder_space_failed(strings::NEW_FOLDER), &e);
            }
        }
        // Shown now, not when the watcher's snapshot comes: with its rules, in its space.
        {
            let mut state = self.state.borrow_mut();
            let mut folders = (*state.folder_paths).clone();
            folders.push(path.clone());
            state.folders = Arc::new(folder_tree_with(state.catalog.iter().map(|e| e.path.as_str()), folders.iter().map(String::as_str)));
            state.folder_paths = Arc::new(folders);
            if let Some(id) = &space {
                let rules = Arc::make_mut(&mut state.rules);
                if let Ok(parsed) = FolderRules::parse(&serde_json::json!({"space": id}).to_string()) {
                    rules.insert(path.clone(), parsed);
                }
                state.spaces = Arc::new(Spaces::from_rules(state.rules.iter().map(|(folder, rules)| (folder.as_str(), rules.object()))));
            }
            state.fit_space();
        }
        self.rebuild_sidebar();
        let item = SidebarItem::Folder(path.clone());
        // Its parents opened, so its row shows.
        let mut at = parent.as_str();
        let mut opened = Vec::new();
        while !at.is_empty() {
            opened.push(at.to_owned());
            at = at.rsplit_once('/').map_or("", |(up, _)| up);
        }
        if self.state.borrow().space.is_some() && !self.state.borrow().space_folders.as_ref().is_some_and(|s| s.holds_folder(&path)) {
            // SAFETY: an item object of this outline view.
            unsafe { self.outline.expandItem(Some(&self.sidebar.object_for(&SidebarItem::Rest))) };
        }
        for folder in opened.iter().rev() {
            // SAFETY: as above.
            unsafe { self.outline.expandItem(Some(&self.sidebar.object_for(&SidebarItem::Folder(folder.clone())))) };
        }
        self.highlight(&item);
        self.select(item.clone());
        // SAFETY: an item object of this outline view.
        let row = unsafe { self.outline.rowForItem(Some(&self.sidebar.object_for(&item))) };
        if row >= 0 {
            self.sidebar.begin_rename(row);
        }
    }

    /// A user edit: autosave once the user pauses.
    pub(super) fn edited(&self) {
        match self.editing.borrow_mut().as_mut() {
            Some(editing) => editing.document.edited(),
            None => return,
        }
        let edit = self.edits.get() + 1;
        self.edits.set(edit);
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let when = DispatchTime::try_from(AUTOSAVE_DELAY).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(window) = target.get(mtm).upgrade()
                && window.edits.get() == edit
            {
                window.save();
            }
        });
    }

    /// File → New Note: an empty `Без назви.md` in the selected folder (the new notes folder
    /// for "All notes" and tags), opened with the cursor in it (ADR 0011, 0013).
    pub fn new_note(&self) {
        let (vault, folder) = {
            let state = self.state.borrow();
            let Some(vault) = state.vault.clone() else { return };
            // The folder selected, else the inbox; in a space one of its folders (ADR 0030).
            let inbox = new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER);
            let folder = match (&state.selection, &state.space_folders) {
                (SidebarItem::Folder(path), _) => path.clone(),
                (SidebarItem::Rest, _) | (_, None) => inbox,
                (_, Some(space)) => space_notes_folder(&inbox, space, &state.folders),
            };
            (vault, folder)
        };
        // The note open is left first: when it stays (its edits unwritten), nothing is made.
        if !self.leave() {
            self.stay();
            return;
        }
        let path = unique_note_path(&folder, strings::UNTITLED, |p| vault.join(p).exists());
        if let Err(e) = FsVault::new(&vault).write_note(&path, "") {
            self.tell_failed(strings::NEW_NOTE_FAILED, &e);
            return;
        }
        if !self.open_note(&path, true) {
            return;
        }
        if let Some(editing) = self.editing.borrow_mut().as_mut() {
            editing.document = Document::untitled();
        }
        self.highlight_open_note();
        self.window.makeFirstResponder(Some(&self.note_text));
    }

    /// Saves and names a new note after its first line: the user leaves the note (another
    /// note opens, the window closes, the app quits). False when its edits could not be
    /// written and the user chose to stay with them: the note must stay where it is.
    pub fn leave(&self) -> bool {
        match self.write_all() {
            Ok(()) => self.name_new_note(true),
            Err((path, e)) => {
                if !self.ask_discard(&path, &e) {
                    return false;
                }
                if let Some(editing) = self.editing.borrow_mut().as_mut() {
                    editing.document.discarded();
                }
                self.embeds.discard();
            }
        }
        self.remember_place();
        self.save_tabs();
        true
    }

    /// Autosave and File → Save; names a new note whose first line is finished. A failure is
    /// told; the edits stay in the window, and leaving the note asks first.
    pub fn save(&self) {
        match self.write_all() {
            Ok(()) => self.name_new_note(false),
            Err((path, e)) => self.tell_save_failed(&path, &e),
        }
    }

    /// Writes the note's edits and those typed in the notes it embeds; the first failure.
    pub(super) fn write_all(&self) -> Result<(), SaveFailure> {
        let note = self.write_edits();
        let embeds = self.write_embeds();
        let written = note.and(embeds);
        if written.is_ok() {
            self.save_failed.set(false);
        }
        written
    }

    /// Says a save failed, once until one works again (autosave would say it every pause).
    pub(super) fn tell_save_failed(&self, path: &str, error: &std::io::Error) {
        eprintln!("knotes: cannot save {path}: {error}");
        if self.save_failed.replace(true) {
            return;
        }
        if probe::enabled() {
            println!("save-failed: {path}: {error}");
            return;
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(&strings::save_failed(knotes_app::title_of(path))));
        alert.setInformativeText(&NSString::from_str(&strings::save_failed_details(&error.to_string())));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.beginSheetModalForWindow_completionHandler(&self.window, None);
    }

    /// The edits of `path` could not be written: whether to leave them (the note, the window,
    /// the app) or to stay, the default. `KNOTES_PROBE_ANSWER=discard` leaves in probe mode.
    fn ask_discard(&self, path: &str, error: &std::io::Error) -> bool {
        eprintln!("knotes: cannot save {path}: {error}");
        if probe::enabled() {
            println!("unsaved-question: {path}: {error}");
            return std::env::var("KNOTES_PROBE_ANSWER").as_deref() == Ok("discard");
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(&strings::save_failed(knotes_app::title_of(path))));
        alert.setInformativeText(&NSString::from_str(&strings::unsaved_details(&error.to_string())));
        alert.addButtonWithTitle(&NSString::from_str(strings::STAY));
        alert.addButtonWithTitle(&NSString::from_str(strings::DISCARD_CHANGES)).setHasDestructiveAction(true);
        alert.runModal() == NSAlertSecondButtonReturn
    }

    /// Says a file operation failed: `title`, and why, in a sheet on the window.
    pub(super) fn tell_failed(&self, title: &str, why: &dyn std::fmt::Display) {
        rename_dialog::tell_failure(objc2::MainThreadOnly::mtm(&*self.window), Some(&self.window), title, &why.to_string());
    }

    /// Says the note at `path` is shown read only and empty: it could not be read (`error`).
    pub(super) fn tell_unreadable(&self, path: &str, error: &std::io::Error) {
        eprintln!("knotes: cannot read {path}: {error}");
        if probe::enabled() {
            println!("unreadable: {path}: {error}");
            return;
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(&strings::unreadable(knotes_app::title_of(path))));
        alert.setInformativeText(&NSString::from_str(&strings::unreadable_details(&error.to_string())));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.beginSheetModalForWindow_completionHandler(&self.window, None);
    }

    /// Says the note at `path` is shown read only: it is not in UTF-8.
    pub(super) fn tell_not_utf8(&self, path: &str) {
        if probe::enabled() {
            println!("read-only: {path}: not UTF-8");
            return;
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(&strings::not_utf8(knotes_app::title_of(path))));
        alert.setInformativeText(&NSString::from_str(strings::NOT_UTF8_DETAILS));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.beginSheetModalForWindow_completionHandler(&self.window, None);
    }

    /// Renames the note at `old` to `name` in its folder (a number added when the name is
    /// taken), with the links to it (ADR 0019); a PDF keeps `.pdf`. False when nothing moved.
    pub fn rename_note(&self, old: &str, name: &str) -> bool {
        // The list renames its PDFs (search results) through here too.
        if FileKind::from_path(old) == FileKind::Pdf {
            return self.rename_pdf(old, name);
        }
        let (Some(stem), Some(vault)) = (note_file_stem(name), self.vault()) else { return false };
        let folder = old.rsplit_once('/').map_or("", |(dir, _)| dir);
        // A change of case only is a rename of the same file.
        let same = |p: &str| p.to_lowercase() == old.to_lowercase();
        let new = unique_note_path(folder, &stem, |p| !same(p) && vault.join(p).exists());
        new != old && self.move_notes(Moves::from([(old.to_owned(), new)]))
    }

    /// Asks for a folder and moves the note at `path` there, with the links to it (ADR 0019).
    pub fn move_note_to_chosen_folder(&self, path: &str) {
        let folders = {
            let state = self.state.borrow();
            let mut list = Vec::new();
            let mut stack = vec![&*state.folders];
            while let Some(node) = stack.pop() {
                list.push(node.path.clone());
                stack.extend(node.children.iter().rev());
            }
            list
        };
        let name = knotes_app::title_of(path).to_owned();
        let Some(folder) = rename_dialog::ask_folder(objc2::MainThreadOnly::mtm(&*self.window), &name, &folders) else { return };
        self.move_note_to(path, &folder);
    }

    /// Moves the note (or PDF) at `path` into `folder` (`""`: the vault's root), with the
    /// links to it; its file name stays, numbered when taken.
    pub fn move_note_to(&self, path: &str, folder: &str) {
        let Some(vault) = self.vault() else { return };
        let file = path.rsplit('/').next().unwrap_or(path);
        let wanted = if folder.is_empty() { file.to_owned() } else { format!("{folder}/{file}") };
        if wanted == path {
            return;
        }
        let new = unique_file_path(folder, file, |p| vault.join(p).exists());
        self.move_notes(Moves::from([(path.to_owned(), new)]));
    }

    /// Renames the folder at `folder` to `name` (a `/` in it becomes `-`), with the links into
    /// it (ADR 0019): the folder itself moves, attachments and all. False when nothing moved.
    pub fn rename_folder(&self, folder: &str, name: &str) -> bool {
        let name = name.trim().replace('/', "-");
        let Some(vault) = self.vault().filter(|_| !name.is_empty()) else { return false };
        let new = match folder.rsplit_once('/') {
            Some((parent, _)) => format!("{parent}/{name}"),
            None => name,
        };
        if new == folder || (new.to_lowercase() != folder.to_lowercase() && vault.join(&new).exists()) {
            return false;
        }
        let prefix = format!("{folder}/");
        let moves: Moves = self
            .state
            .borrow()
            .catalog
            .iter()
            .filter_map(|e| e.path.strip_prefix(&prefix).map(|rest| (e.path.clone(), format!("{new}/{rest}"))))
            .collect();
        if !self.relocate(moves, Some((folder, &new))) {
            return false;
        }
        // The sidebar keeps showing the folder (or the folder inside it) under its new name.
        let selected = match &self.state.borrow().selection {
            SidebarItem::Folder(path) if path == folder => Some(new.clone()),
            SidebarItem::Folder(path) => path.strip_prefix(&prefix).map(|rest| format!("{new}/{rest}")),
            _ => None,
        };
        if let Some(path) = selected {
            self.state.borrow_mut().selection = SidebarItem::Folder(path);
        }
        true
    }

    /// Moves notes (new path by old), asking first when links would change and rewriting them
    /// on yes (ADR 0019). The open note is saved first and follows its file. False when
    /// nothing moved.
    pub fn move_notes(&self, moves: Moves) -> bool {
        self.relocate(moves, None)
    }

    /// `move_notes`, the files moved one by one, or all at once by moving `folder` (from, to).
    pub(super) fn relocate(&self, moves: Moves, folder: Option<(&str, &str)>) -> bool {
        let Some(vault_dir) = self.vault() else { return false };
        // Written first, the embedded notes' edits too, so the links are planned on what the
        // notes hold. Unwritten edits stay in the editor and follow the note to its new path.
        let _ = self.write_all();
        let parent = |path: &str| path.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_owned());
        let plan = match self.relinks(&moves) {
            Ok(plan) => plan,
            // Moved without them, the links to it would break.
            Err(e) => {
                let name = folder.map(|(from, _)| from.rsplit('/').next().unwrap_or(from).to_owned()).or_else(|| moves.keys().next().map(|p| document_name(p).to_owned()));
                self.tell_failed(&strings::links_unknown(&name.unwrap_or_default()), &strings::links_unknown_details(&e));
                return false;
            }
        };
        let update = plan.links == 0
            || match rename_dialog::ask_link_update(objc2::MainThreadOnly::mtm(&*self.window), plan.links, plan.edits.len()) {
                LinkUpdate::Update => true,
                LinkUpdate::Keep => false,
                LinkUpdate::Cancel => return false,
            };
        let vault = FsVault::new(&vault_dir);
        let mut moved = Moves::new();
        if let Some((from, to)) = folder {
            match vault.move_note(from, to) {
                Ok(()) => moved = moves.clone(),
                Err(e) => {
                    let name = from.rsplit('/').next().unwrap_or(from);
                    self.tell_failed(&if parent(from) == parent(to) { strings::rename_failed(name) } else { strings::move_failed(name) }, &e);
                    return false;
                }
            }
        }
        let mut failures = Vec::new();
        for (from, to) in moves.iter().filter(|_| folder.is_none()) {
            match vault.move_note(from, to) {
                Ok(()) => {
                    moved.insert(from.clone(), to.clone());
                }
                Err(e) => failures.push((from.as_str(), to.as_str(), e)),
            }
        }
        if let Some((from, to, e)) = failures.first() {
            let title = match failures.len() {
                1 if parent(from) == parent(to) => strings::rename_failed(document_name(from)),
                1 => strings::move_failed(document_name(from)),
                _ => strings::MOVES_FAILED.to_owned(),
            };
            let names: Vec<&str> = failures.iter().skip(usize::from(failures.len() == 1)).map(|(from, _, _)| document_name(from)).collect();
            self.tell_failed(&title, &strings::failed_files(&e.to_string(), &names));
        }
        if moved.is_empty() && folder.is_none() {
            return false;
        }
        let failed = |path: &str| moves.iter().any(|(from, to)| to == path && !moved.contains_key(from));
        let edits: Vec<&(String, String)> = if update { plan.edits.iter().filter(|(path, _)| !failed(path)).collect() } else { Vec::new() };
        // The files moved: a note whose links could not be updated is said, its links broken.
        let unlinked: Vec<(&str, std::io::Error)> = edits.iter().filter_map(|(path, text)| vault.write_note(path, text).err().map(|e| (path.as_str(), e))).collect();
        if let Some((_, e)) = unlinked.first() {
            let names: Vec<&str> = unlinked.iter().map(|(path, _)| document_name(path)).collect();
            self.tell_failed(strings::LINKS_FAILED, &strings::failed_files(&e.to_string(), &names));
        }
        for (from, to) in &moved {
            self.state.borrow_mut().history.rename(from, to);
            self.tabs.borrow_mut().rename(from, to);
        }
        self.save_tabs();
        if let Some(open) = self.open_path() {
            let path = moved.get(&open).cloned().unwrap_or(open);
            let edited = edits.iter().find(|(p, _)| *p == path).map(|(_, text)| text.clone());
            if let Some(editing) = self.editing.borrow_mut().as_mut() {
                editing.path = path.clone();
                if let Some(text) = &edited {
                    editing.document.saved(text);
                }
            }
            self.live_preview.set_path(&path);
            if let Some(text) = edited {
                let cursor = self.note_text.selectedRange();
                self.live_preview.show(&self.note_text, &path, &text);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
            }
        }
        // The PDF open moved: shown from its new place, at the page it was at.
        let pdf = self.pdf_path.borrow().clone();
        if let Some(new) = pdf.and_then(|p| moved.get(&p).cloned()) {
            let page = self.pdf_view.borrow().as_ref().map_or(1, |v| v.page().0);
            self.show_pdf(Some(&new));
            if let Some(viewer) = self.pdf_view.borrow().as_ref() {
                self.window.layoutIfNeeded();
                viewer.go_to(page.saturating_sub(1));
            }
            self.refresh_panel();
        }
        self.update_navigation();
        true
    }

    /// Renames the PDF at `old` to `name` (`.pdf` added when left out) in its folder, links to
    /// it updated as a note's are; false when declined or impossible.
    pub fn rename_pdf(&self, old: &str, name: &str) -> bool {
        let name = name.trim();
        let stem = if name.to_lowercase().ends_with(".pdf") { &name[..name.len() - 4] } else { name };
        let (Some(stem), Some(vault)) = (note_file_stem(stem), self.vault()) else { return false };
        let folder = old.rsplit_once('/').map_or(String::new(), |(dir, _)| format!("{dir}/"));
        // A change of case only is a rename of the same file.
        let free = |p: &str| p.to_lowercase() == old.to_lowercase() || !vault.join(p).exists();
        let new = (0..)
            .map(|n| if n == 0 { format!("{folder}{stem}.pdf") } else { format!("{folder}{stem} {n}.pdf") })
            .find(|p| free(p))
            .unwrap_or_default();
        new != old && self.move_notes(Moves::from([(old.to_owned(), new)]))
    }

    /// The link edits `moves` call for, worked out from the index and the files; why not, when
    /// the index cannot say which notes link to them.
    pub(super) fn relinks(&self, moves: &Moves) -> Result<Relinks, String> {
        // Just written (`relocate` writes them first), their newest links not indexed yet.
        let fresh: Vec<String> = self.open_path().into_iter().chain(self.embeds.paths()).collect();
        let mut state = self.state.borrow_mut();
        self.ensure_reader(&mut state);
        let (Some(reader), Some(before), Some(vault)) = (state.reader.as_ref(), state.links.clone(), state.vault.clone()) else {
            return Ok(Relinks::default());
        };
        let after = LinkTargets::new(state.catalog.iter().map(|e| moves.get(&e.path).map_or(e.path.as_str(), String::as_str)));
        let files = FsVault::new(&vault);
        // A note not in UTF-8 is left as it is: written back, it would lose its bytes.
        plan_relinks(moves, reader, &|p| files.read_utf8(p).ok(), &MarkdownParser::new(), &*before, &after, &fresh).map_err(|e| e.to_string())
    }

    /// Renames a new note's file after its first line, once (ADR 0011).
    pub(super) fn name_new_note(&self, leaving: bool) {
        let text = self.note_text.string().to_string();
        let old = match self.editing.borrow().as_ref() {
            Some(editing) if editing.document.wants_name(&text, leaving) => editing.path.clone(),
            _ => return,
        };
        let Some(stem) = note_file_stem(text.lines().next().unwrap_or("")) else { return };
        let Some(vault) = self.vault() else { return };
        let folder = old.rsplit_once('/').map_or("", |(dir, _)| dir);
        let path = unique_note_path(folder, &stem, |p| p != old && vault.join(p).exists());
        if path != old
            && let Err(e) = FsVault::new(&vault).move_note(&old, &path)
        {
            // Not tried again at each save: the note keeps its file's name.
            if let Some(editing) = self.editing.borrow_mut().as_mut() {
                editing.document.named();
            }
            self.tell_failed(&strings::name_failed(&stem), &e);
            return;
        }
        if let Some(editing) = self.editing.borrow_mut().as_mut() {
            editing.path = path.clone();
            editing.document.named();
        }
        self.state.borrow_mut().history.rename(&old, &path);
        // Its tab follows the file (the next snapshot drops tabs whose files are gone).
        self.tabs.borrow_mut().rename(&old, &path);
        self.save_tabs();
        self.live_preview.set_path(&path);
    }

    /// Writes the editor's text if it has unsaved edits. A change on disk the window has not
    /// seen yet is kept next to the note as a conflict copy instead of being overwritten. A note
    /// gone from the disk (deleted or moved outside Knotes) is not made again where it was: the
    /// edits go beside that place, as a copy, and the note turns read only until it closes.
    /// The error when the text could not be written (it stays unsaved in the editor).
    pub(super) fn write_edits(&self) -> Result<(), SaveFailure> {
        let Some(vault) = self.state.borrow().vault.clone() else { return Ok(()) };
        let (copy, written) = {
            let mut editing = self.editing.borrow_mut();
            let Some(editing) = editing.as_mut().filter(|e| e.document.is_dirty() && !e.read_only) else { return Ok(()) };
            let vault = FsVault::new(&vault);
            let text = self.note_text.string().to_string();
            let mut copy = None;
            match vault.read_text(&editing.path) {
                Ok(disk) => match editing.document.on_disk(&disk, &text) {
                    // The version from outside kept beside it, or not overwritten: the edits stay unsaved.
                    DiskChange::Conflict => match keep_conflict_copy(&vault, &editing.path, &disk) {
                        Ok(kept) => copy = Some((kept, false)),
                        Err(e) => return Err((editing.path.clone(), e)),
                    },
                    DiskChange::Ignore if !editing.document.is_dirty() => return Ok(()),
                    _ => {}
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let kept = match keep_conflict_copy(&vault, &editing.path, &text) {
                        Ok(kept) => kept,
                        Err(copy_error) => return Err((editing.path.clone(), if copy_error.kind() == e.kind() { e } else { copy_error })),
                    };
                    editing.document.discarded();
                    editing.read_only = true;
                    copy = Some((kept, true));
                }
                Err(_) => {}
            }
            let written = match copy {
                Some((_, true)) => Ok(()),
                _ => write(&vault, editing, &text).map_err(|e| (editing.path.clone(), e)),
            };
            (copy.map(|(copy, gone)| (editing.path.clone(), copy, gone)), written)
        };
        match copy {
            Some((_, copy, true)) => {
                self.show_note_column();
                self.tell_copy(strings::GONE_TITLE, &strings::gone_details(knotes_app::title_of(&copy)), &copy, "vanished-notice");
            }
            Some((path, copy, false)) => self.tell_conflict(&path, &copy),
            None => {}
        }
        written
    }

    /// The note open is gone from the disk: deleted, or moved outside Knotes (its new name is
    /// not known). Edits not written yet go beside where it was (`write_edits`), and it closes;
    /// when even that copy cannot be written it stays, its edits in it.
    pub(super) fn note_vanished(&self) {
        match self.write_edits() {
            Ok(()) => {
                self.show_nothing();
            }
            Err((path, e)) => self.tell_save_failed(&path, &e),
        }
    }

    /// Tells where the other version of a note went (ADR 0011), in a sheet on the window that
    /// can open it.
    pub(super) fn tell_conflict(&self, path: &str, copy: &str) {
        self.tell_copy(strings::CONFLICT_TITLE, &strings::conflict_details(knotes_app::title_of(path), knotes_app::title_of(copy)), copy, "conflict-notice");
    }

    /// A sheet saying `title` and `details` about the copy at `copy`, with a button opening it;
    /// in probe mode the line `probe: copy`.
    pub(super) fn tell_copy(&self, title: &str, details: &str, copy: &str, probe: &str) {
        if probe::enabled() {
            println!("{probe}: {copy}");
            return;
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(details));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OPEN_COPY));
        let weak = self.this.borrow().clone();
        let copy = copy.to_owned();
        let handler = block2::RcBlock::new(move |response: NSModalResponse| {
            if response == NSAlertSecondButtonReturn
                && let Some(window) = weak.upgrade()
            {
                window.open_note(&copy, true);
            }
        });
        alert.beginSheetModalForWindow_completionHandler(&self.window, Some(&handler));
    }

    /// The open note changed on disk: our own save is ignored, a clean editor reloads, and
    /// edits on both sides keep both versions.
    pub(super) fn changed_on_disk(&self, path: &str) {
        let Some(vault) = self.state.borrow().vault.clone() else { return };
        let (disk, read_only) = match read_for_editor(&FsVault::new(&vault), path) {
            NoteRead::Text(text) => (text, false),
            NoteRead::NotUtf8(text) => (text, true),
            // Not readable now: what is shown stays.
            NoteRead::Failed(_) => return,
        };
        let text = self.note_text.string().to_string();
        let change = match self.editing.borrow_mut().as_mut() {
            Some(editing) => editing.document.on_disk(&disk, &text),
            None => return,
        };
        match change {
            DiskChange::Ignore => {}
            DiskChange::Reload => {
                let cursor = self.note_text.selectedRange();
                self.live_preview.show(&self.note_text, path, &disk);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
                // Written outside in another encoding (or back in UTF-8): editable as it is now.
                let was = self.editing.borrow_mut().as_mut().map(|e| std::mem::replace(&mut e.read_only, read_only));
                if was == Some(!read_only) {
                    self.show_note_column();
                    if read_only {
                        self.tell_not_utf8(path);
                    }
                }
            }
            DiskChange::Conflict => {
                // The editor's text keeps the note's name; the copy holds the other side.
                let vault = FsVault::new(&vault);
                let copy = match keep_conflict_copy(&vault, path, &disk) {
                    Ok(copy) => copy,
                    // Not kept, so not overwritten: the editor's edits stay unsaved (leaving asks).
                    Err(e) => {
                        self.tell_save_failed(path, &e);
                        return;
                    }
                };
                let written = self.editing.borrow_mut().as_mut().map(|editing| write(&vault, editing, &text));
                if let Some(Err(e)) = written {
                    self.tell_save_failed(path, &e);
                }
                self.tell_conflict(path, &copy);
            }
        }
    }
}
