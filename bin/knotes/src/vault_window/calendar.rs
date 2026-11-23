//! The calendar of daily notes (the right panel): a note named by its day is that day's, and a
//! click on a day opens it, or makes it in the vault's folder of daily notes.

use knotes_app::{Day, daily_day, daily_moves, daily_note_path, day_at};
use knotes_core::FolderRulesStore;

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

    /// The settings' «Перекласти наявні нотатки…»: every daily note of the vault moved where the
    /// daily notes' folder and layout put it now, once the user says so, the links to them
    /// updated as for any move. The rules are read from the root's file: the settings wrote
    /// them a moment ago.
    pub fn relayout_daily_notes(&self) {
        let Some(vault) = self.vault() else { return };
        let mtm = MainThreadMarker::from(&*self.window);
        let text = FsVault::new(&vault).read_rules(&[""]).ok().and_then(|found| found.into_iter().next()).map_or_else(|| "{}".to_owned(), |(_, text)| text);
        let rules: HashMap<String, FolderRules> = FolderRules::parse(&text).ok().map(|rules| (String::new(), rules)).into_iter().collect();
        let (moves, staying) = daily_moves(self.state.borrow().catalog.iter().map(|entry| entry.path.as_str()), &rules, strings::DAILY_FOLDER);
        let taken = |window: &Self| {
            if staying > 0 {
                window.tell_failed(&strings::daily_relayout_staying(staying), &strings::DAILY_RELAYOUT_TAKEN);
            }
        };
        let Some(example) = moves.values().min().cloned() else {
            if staying == 0 {
                self.tell_failed(strings::DAILY_RELAYOUT_NONE, &"");
            }
            return taken(self);
        };
        if crate::probe::enabled() {
            println!("daily-relayout-question: {}", strings::daily_relayout_question(moves.len(), &example));
            if std::env::var("KNOTES_PROBE_ANSWER").as_deref() == Ok("cancel") {
                return;
            }
        } else {
            let alert = objc2_app_kit::NSAlert::new(mtm);
            alert.setMessageText(&NSString::from_str(&strings::daily_relayout_question(moves.len(), &example)));
            alert.setInformativeText(&NSString::from_str(strings::DAILY_RELAYOUT_EXPLAINED));
            for title in [strings::DAILY_RELAYOUT_DO, strings::CANCEL] {
                alert.addButtonWithTitle(&NSString::from_str(title));
            }
            if alert.runModal() != objc2_app_kit::NSAlertFirstButtonReturn {
                return;
            }
        }
        self.move_notes(moves);
        taken(self);
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
