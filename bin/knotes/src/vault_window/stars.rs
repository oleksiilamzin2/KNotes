//! The stars: the folders and files the user starred, listed in the right panel. They are kept
//! in the vault root's rules file, so the snapshot that follows a write shows them.

use knotes_app::{moved_stars, starred_folder, toggled_star, with_stars};

use super::*;
use crate::stars_panel::StarRow;

impl VaultWindow {
    /// The vault's stars as its root's rules file lists them now.
    pub(super) fn set_stars(&self, stars: Vec<String>) {
        if **self.stars.borrow() != stars {
            let stars = Arc::new(stars);
            self.list.set_stars(Arc::clone(&stars));
            self.sidebar.set_stars(Arc::clone(&stars));
            *self.stars.borrow_mut() = stars;
        }
        self.refresh_stars();
    }

    /// Lists the stars, when the panel shows them: those whose folder or file is there (one
    /// deleted keeps its star for when it is restored).
    pub(super) fn refresh_stars(&self) {
        if self.panel_mode.get() != PanelMode::Stars {
            return;
        }
        let Some(vault) = self.vault() else { return self.stars_panel.show(Vec::new()) };
        let rows = self
            .stars
            .borrow()
            .iter()
            .filter_map(|star| match starred_folder(star) {
                Some(folder) => vault.join(folder).is_dir().then(|| StarRow { star: star.clone(), name: folder.rsplit('/').next().unwrap_or(folder).to_owned() }),
                None => vault.join(star).is_file().then(|| StarRow { star: star.clone(), name: document_name(star).to_owned() }),
            })
            .collect();
        self.stars_panel.show(rows);
    }

    /// A click on a star: its folder shown in the sidebar and the list, or its file opened.
    pub(super) fn open_star(&self, star: &str) {
        match starred_folder(star) {
            Some(folder) => self.show_folder(folder),
            None => {
                self.open_note(star, true);
            }
        }
    }

    /// Stars the folder or file `star` names, or takes its star off.
    pub(super) fn toggle_star(&self, star: &str) {
        let stars = toggled_star(self.stars.borrow().to_vec(), star);
        self.write_stars(stars);
    }

    /// The stars follow what moved (`moved`: new path by old; `folder`: a folder moved whole).
    pub(super) fn carry_stars(&self, moved: &Moves, folder: Option<(&str, &str)>) {
        let stars = moved_stars(&self.stars.borrow(), moved, folder);
        if stars != **self.stars.borrow() {
            self.write_stars(stars);
        }
    }

    /// Writes `stars` into the root's rules file and shows them at once; said when it fails.
    fn write_stars(&self, stars: Vec<String>) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        let written = files
            .read_rules(&[""])
            .map_err(|e| e.to_string())
            .and_then(|found| with_stars(&found.into_iter().next().map(|(_, text)| text).unwrap_or_default(), &stars))
            .and_then(|text| files.write_rules("", &text).map_err(|e| e.to_string()));
        match written {
            Ok(()) => self.set_stars(stars),
            Err(e) => self.tell_failed(strings::STAR_FAILED, &strings::failed_files(&e, &[knotes_app::RULES_FILE])),
        }
    }
}
