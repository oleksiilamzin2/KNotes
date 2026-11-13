//! The question before a rename changes links in other notes (ADR 0019).

use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn};
use objc2_foundation::NSString;

use crate::{probe, strings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkUpdate {
    Update,
    Keep,
    Cancel,
}

/// Asks whether to update `links` links in `notes` notes. In probe mode the answer is
/// `KNOTES_PROBE_ANSWER` (`update`, `keep` or `cancel`; update by default).
pub fn ask_link_update(mtm: MainThreadMarker, links: usize, notes: usize) -> LinkUpdate {
    if probe::enabled() {
        println!("rename-question: {links} {notes}");
        return match std::env::var("KNOTES_PROBE_ANSWER").as_deref() {
            Ok("keep") => LinkUpdate::Keep,
            Ok("cancel") => LinkUpdate::Cancel,
            _ => LinkUpdate::Update,
        };
    }
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&strings::rename_question(links, notes)));
    alert.setInformativeText(&NSString::from_str(strings::RENAME_KEEP_EXPLAINED));
    for title in [strings::RENAME_UPDATE, strings::RENAME_KEEP, strings::CANCEL] {
        alert.addButtonWithTitle(&NSString::from_str(title));
    }
    match alert.runModal() {
        r if r == NSAlertFirstButtonReturn => LinkUpdate::Update,
        r if r == NSAlertSecondButtonReturn => LinkUpdate::Keep,
        _ => LinkUpdate::Cancel,
    }
}
