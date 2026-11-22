//! The calendar of daily notes (the right panel): a note named by its day is that day's, and a
//! click on a day opens it, or makes it in the vault's folder of daily notes.

use knotes_app::{Day, daily_day, daily_note_path, day_at};

use super::*;

impl VaultWindow {
    /// The day it is and the days that have a note, when the panel shows the calendar.
    pub(super) fn refresh_calendar(&self) {
        if self.panel_mode.get() != PanelMode::Calendar {
            return;
        }
        let offset = objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT() as i32;
        let noted = self.state.borrow().catalog.iter().filter_map(|entry| daily_day(&entry.path)).collect();
        self.calendar_panel.show(day_at(knotes_session::now(), offset), noted);
    }

    /// A click on a day: its note opened (the first by path, when several are named by it), or
    /// made empty in the folder of daily notes and opened.
    pub(super) fn open_day(&self, day: Day) {
        let (vault, path, made) = {
            let state = self.state.borrow();
            let Some(vault) = state.vault.clone() else { return };
            let there = state.catalog.iter().map(|entry| entry.path.as_str()).filter(|path| daily_day(path) == Some(day)).min().map(str::to_owned);
            let made = there.is_none();
            (vault, there.unwrap_or_else(|| daily_note_path(&state.rules, strings::DAILY_FOLDER, day)), made)
        };
        // One made a moment ago (not in the catalog yet) is opened as it is.
        if made
            && !vault.join(&path).exists()
            && let Err(e) = FsVault::new(&vault).write_note(&path, "")
        {
            self.tell_failed(&strings::create_failed(document_name(&path)), &e);
            return;
        }
        if self.open_note(&path, true) {
            self.highlight_open_note();
        }
    }
}
