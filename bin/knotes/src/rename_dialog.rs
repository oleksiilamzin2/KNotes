//! The questions of renaming and moving notes (ADR 0019): which folder to move a note to, and
//! whether to update the links a rename changes in other notes.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSComboBox, NSTextField, NSWindow};
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

/// Asks a new space's name (ADR 0030); `None` when cancelled or left empty. In probe mode the
/// name comes from `KNOTES_PROBE_SPACE_NAME`.
pub fn ask_space_name(mtm: MainThreadMarker) -> Option<String> {
    if probe::enabled() {
        return std::env::var("KNOTES_PROBE_SPACE_NAME").ok().filter(|n| !n.trim().is_empty());
    }
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(strings::NEW_SPACE_QUESTION));
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(260.0, 24.0)));
    alert.setAccessoryView(Some(&field));
    alert.window().setInitialFirstResponder(Some(&field));
    for title in [strings::CREATE, strings::CANCEL] {
        alert.addButtonWithTitle(&NSString::from_str(title));
    }
    (alert.runModal() == NSAlertFirstButtonReturn).then(|| field.stringValue().to_string().trim().to_owned()).filter(|n| !n.is_empty())
}

/// Asks a new card's deck (`decks` to choose from, `last` offered) and, when `need_question`,
/// its question; none when cancelled or left without a deck. In probe mode
/// `KNOTES_PROBE_CARD=deck|question`.
pub fn ask_card(mtm: MainThreadMarker, decks: &[String], last: &str, need_question: bool) -> Option<(String, Option<String>)> {
    let answer = |deck: String, question: String| {
        let deck = deck.trim().trim_matches('/').to_owned();
        let question = Some(question.trim().to_owned()).filter(|q| !q.is_empty());
        (!deck.is_empty() && (question.is_some() || !need_question)).then_some((deck, question.filter(|_| need_question)))
    };
    if probe::enabled() {
        let asked = std::env::var("KNOTES_PROBE_CARD").ok()?;
        let (deck, question) = asked.split_once('|').unwrap_or((&asked, ""));
        return answer(deck.to_owned(), question.to_owned());
    }
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(strings::CARD_QUESTION));
    let deck = objc2_app_kit::NSComboBox::initWithFrame(objc2_app_kit::NSComboBox::alloc(mtm), NSRect::new(NSPoint::new(0.0, 32.0), NSSize::new(320.0, 26.0)));
    deck.setPlaceholderString(Some(&NSString::from_str(strings::CARD_DECK)));
    for name in decks {
        // SAFETY: a string, as a combo box's items are.
        unsafe { deck.addItemWithObjectValue(&NSString::from_str(name)) };
    }
    deck.setCompletes(true);
    deck.setStringValue(&NSString::from_str(last));
    let question = NSTextField::initWithFrame(NSTextField::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 24.0)));
    question.setPlaceholderString(Some(&NSString::from_str(strings::CARD_ASKS)));
    question.setHidden(!need_question);
    let fields = objc2_app_kit::NSView::initWithFrame(objc2_app_kit::NSView::alloc(mtm), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 60.0)));
    fields.addSubview(&deck);
    fields.addSubview(&question);
    alert.setAccessoryView(Some(&fields));
    // The question first when the deck is known already.
    if need_question && !last.is_empty() {
        alert.window().setInitialFirstResponder(Some(&question));
    } else {
        alert.window().setInitialFirstResponder(Some(&deck));
    }
    // SAFETY: both fields live in the alert's accessory view.
    unsafe { deck.setNextKeyView(Some(&question)) };
    for title in [strings::CREATE, strings::CANCEL] {
        alert.addButtonWithTitle(&NSString::from_str(title));
    }
    (alert.runModal() == NSAlertFirstButtonReturn).then(|| answer(deck.stringValue().to_string(), question.stringValue().to_string())).flatten()
}

/// A typed folder as a vault path: no slashes at its ends, `/` the root.
fn folder_of(typed: &str) -> String {
    typed.trim().trim_matches('/').to_owned()
}

/// Says a file operation failed: `title`, and why (`details`), in a sheet on `window`, or in a
/// dialog of its own without one. In probe mode the line `failed: title: details`.
pub fn tell_failure(mtm: MainThreadMarker, window: Option<&NSWindow>, title: &str, details: &str) {
    eprintln!("knotes: {title}: {details}");
    if probe::enabled() {
        println!("failed: {title}: {details}");
        return;
    }
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(details));
    alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
    match window {
        Some(window) => alert.beginSheetModalForWindow_completionHandler(window, None),
        None => {
            alert.runModal();
        }
    }
}
