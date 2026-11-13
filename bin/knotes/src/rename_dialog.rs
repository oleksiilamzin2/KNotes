//! The questions of renaming and moving notes (ADR 0019): which folder to move a note to, and
//! whether to update the links a rename changes in other notes.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSComboBox};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

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

/// Asks which folder to move the note `note` to: one of `folders` (`""` is the vault's root,
/// shown as `/`), completed as it is typed, or a new one. `None` when cancelled. In probe
/// mode the folder comes from `KNOTES_PROBE_ACTION=list-move=folder`.
pub fn ask_folder(mtm: MainThreadMarker, note: &str, folders: &[String]) -> Option<String> {
    let shown: Vec<&str> = folders.iter().map(|f| if f.is_empty() { strings::ROOT_FOLDER } else { f.as_str() }).collect();
    if probe::enabled() {
        println!("move-folders: {}", shown.join(" | "));
        return std::env::var("KNOTES_PROBE_ACTION").ok()?.strip_prefix("list-move=").map(folder_of);
    }
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&strings::move_question(note)));
    let combo = NSComboBox::initWithFrame(NSComboBox::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 26.0)));
    combo.setCompletes(true);
    combo.setNumberOfVisibleItems(12);
    let values: Vec<Retained<AnyObject>> = shown.iter().map(|f| Retained::into_super(Retained::into_super(NSString::from_str(f)))).collect();
    // SAFETY: strings, the values a combo box shows.
    unsafe { combo.addItemsWithObjectValues(&NSArray::from_retained_slice(&values)) };
    alert.setAccessoryView(Some(&combo));
    alert.window().setInitialFirstResponder(Some(&combo));
    for title in [strings::MOVE, strings::CANCEL] {
        alert.addButtonWithTitle(&NSString::from_str(title));
    }
    (alert.runModal() == NSAlertFirstButtonReturn).then(|| folder_of(&combo.stringValue().to_string()))
}

/// A typed folder as a vault path: no slashes at its ends, `/` the root.
fn folder_of(typed: &str) -> String {
    typed.trim().trim_matches('/').to_owned()
}
