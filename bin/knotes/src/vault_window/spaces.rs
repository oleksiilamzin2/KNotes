//! Spaces (ADR 0030): the window's space, a folder's, the switcher, the space remembered beside the index.

use super::*;

/// The window's space, beside the index (ADR 0030).
pub(super) const SPACE_FILE: &str = "space.json";

impl VaultWindow {
    /// The window's space (ADR 0030), none for the whole vault: kept beside the index; the
    /// sidebar and the list are shown again for it (a selection outside it goes to all notes).
    pub fn set_space(&self, id: Option<String>) {
        {
            let mut state = self.state.borrow_mut();
            state.space = id.filter(|id| state.spaces.get(id).is_some());
            state.fit_space();
        }
        self.save_space();
        self.show_spaces();
        // The space's decks: those of its notes' cards (ADR 0037).
        self.recount_decks();
        // The tabs' dots: of the spaces that are not the window's now.
        self.show_tabs();
        self.rebuild_sidebar();
        let selection = self.state.borrow_mut().fit_selection(strings::NEW_NOTES_FOLDER);
        self.highlight(&selection);
        self.refresh_list();
    }

    /// A folder's menu put it in a space, in none, or in a new space named in a dialog (ADR
    /// 0030): its rules file says so only where that differs from its parent's space (else the
    /// key goes). The snapshot that follows the writes shows it.
    pub(super) fn set_folder_space(&self, folder: &str, choice: FolderChoice) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        // A rules file that cannot be read is not written over as if it were empty.
        let rules_of = |folder: &str| files.read_rules(&[folder]).map(|r| r.into_iter().next().map(|(_, text)| text).unwrap_or_default()).map_err(|e| e.to_string());
        let write = |folder: &str, text: Result<String, String>| match text.map_err(std::io::Error::other).and_then(|text| files.write_rules(folder, &text)) {
            Ok(()) => true,
            Err(e) => {
                let file = if folder.is_empty() { knotes_app::RULES_FILE.to_owned() } else { format!("{folder}/{}", knotes_app::RULES_FILE) };
                self.tell_failed(&strings::folder_space_failed(folder.rsplit('/').next().unwrap_or(folder)), &strings::failed_files(&e.to_string(), &[&file]));
                false
            }
        };
        let spaces = Arc::clone(&self.state.borrow().spaces);
        let parent = folder.rsplit_once('/').map_or("", |(parent, _)| parent);
        // Every space: said here unless the parent says it already.
        if choice == FolderChoice::EverySpace {
            write(folder, rules_of(folder).and_then(|text| set_folder_space(&text, spaces.says_for_every(parent))));
            return;
        }
        let id = match choice {
            FolderChoice::Space(id) => Some(id),
            FolderChoice::NoSpace | FolderChoice::EverySpace => None,
            FolderChoice::NewSpace => {
                let Some(name) = rename_dialog::ask_space_name(MainThreadMarker::from(&*self.window)) else { return };
                let mut list = spaces.list().to_vec();
                let made = knotes_spaces::new_space(&list, &name);
                let id = made.id.clone();
                list.push(made);
                if !write("", rules_of("").and_then(|text| set_spaces(&text, &list))) {
                    return;
                }
                Some(id)
            }
        };
        // The parent's space, unless it is in every space (then this folder must say its own).
        let wanted = spaces.says_for(parent, id.as_deref());
        write(folder, rules_of(folder).and_then(|text| set_folder_space(&text, wanted)));
    }

    /// The switcher offers the vault's spaces, the window's selected; the list and the sidebar
    /// mark the others'.
    pub(super) fn show_spaces(&self) {
        let state = self.state.borrow();
        self.space_switcher.set_spaces(state.spaces.list(), state.space.as_deref());
        self.list.set_spaces(Arc::clone(&state.spaces), state.space.clone());
        self.sidebar.set_spaces(Arc::clone(&state.spaces), state.space.clone());
        self.chat.set_space(state.space.as_ref().and_then(|id| state.spaces.get(id)).map(|s| (s.id.clone(), s.name.clone())));
    }

    /// The list's marked notes, for the probe.
    pub fn probe_list_marks(&self) -> String {
        self.list.probe_marks()
    }

    /// The switcher's menu, for the probe.
    pub fn probe_space_menu(&self) -> String {
        self.space_switcher.probe_menu()
    }

    /// The space and the spaces, for the probe: "Робота | spaces: Робота, Дім" (— for none).
    pub fn describe_space(&self) -> String {
        let state = self.state.borrow();
        let name = state.space.as_ref().and_then(|id| state.spaces.get(id)).map_or("—", |s| s.name.as_str());
        let names: Vec<&str> = state.spaces.list().iter().map(|s| s.name.as_str()).collect();
        format!("{name} | spaces: {}", names.join(", "))
    }

    pub(super) fn space_file(&self) -> Option<PathBuf> {
        self.state.borrow().index_dir.as_ref().map(|dir| dir.join(SPACE_FILE))
    }

    pub(super) fn save_space(&self) {
        let Some(file) = self.space_file() else { return };
        let text = serde_json::json!({"space": self.state.borrow().space}).to_string();
        let tmp = file.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &file)) {
            eprintln!("knotes: cannot save the space: {e}");
        }
    }

    /// The space of the last launch.
    pub(super) fn saved_space(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.space_file()?).ok()?;
        serde_json::from_str::<serde_json::Value>(&text).ok()?.get("space")?.as_str().map(str::to_owned)
    }
}
