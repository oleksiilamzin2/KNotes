//! The settings window (Cmd+,), a tab a subject (ADR 0029): «Пошук» chooses the model search by
//! meaning runs on, or turns search by meaning off.

use std::cell::RefCell;

use knotes_vectors::MeaningModel;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSFont, NSImage, NSLayoutConstraint, NSMenuItem, NSPopUpButton, NSTabViewController, NSTabViewControllerTabStyle, NSTabViewItem, NSTextField, NSView, NSViewController, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSRect, NSString};

use crate::strings;

/// The tag of the item turning search by meaning off (a model's is its place in
/// `MeaningModel::ALL`).
const OFF_TAG: isize = 99;

type OnMeaning = Box<dyn Fn()>;

struct Settings {
    window: Retained<NSWindow>,
    popup: Retained<NSPopUpButton>,
    _tabs: Retained<NSTabViewController>,
    _target: Retained<SettingsTarget>,
}

thread_local! {
    static SHOWN: RefCell<Option<Settings>> = const { RefCell::new(None) };
    static ON_MEANING: RefCell<Option<OnMeaning>> = const { RefCell::new(None) };
}

define_class!(
    /// The settings' controls' target.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSettingsTarget"]
    struct SettingsTarget;

    unsafe impl NSObjectProtocol for SettingsTarget {}

    impl SettingsTarget {
        #[unsafe(method(chooseMeaning:))]
        fn choose_meaning(&self, sender: &NSPopUpButton) {
            chosen(sender.selectedTag());
        }
    }
);

impl SettingsTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }
}

/// Runs after the user chose another model for search by meaning (or none).
pub fn on_meaning_change(callback: impl Fn() + 'static) {
    ON_MEANING.with(|c| *c.borrow_mut() = Some(Box::new(callback)));
}

/// Shows the window (made at the first call) with the current choices.
pub fn show(mtm: MainThreadMarker) {
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        let settings = shown.get_or_insert_with(|| make(mtm));
        let tag = crate::meaning::model().and_then(|m| MeaningModel::ALL.iter().position(|a| *a == m)).map_or(OFF_TAG, |i| i as isize);
        settings.popup.selectItemWithTag(tag);
        settings.window.makeKeyAndOrderFront(None);
    });
}

fn chosen(tag: isize) {
    let model = if tag == OFF_TAG { None } else { usize::try_from(tag).ok().and_then(|i| MeaningModel::ALL.get(i).copied()) };
    if model == crate::meaning::model() {
        return;
    }
    crate::meaning::choose(model);
    ON_MEANING.with(|c| {
        if let Some(callback) = c.borrow().as_ref() {
            callback();
        }
    });
}

fn title_of(model: MeaningModel) -> &'static str {
    match model {
        MeaningModel::E5Large => strings::SETTINGS_MEANING_E5,
        MeaningModel::EmbeddingGemma2 => strings::SETTINGS_MEANING_GEMMA,
    }
}

fn make(mtm: MainThreadMarker) -> Settings {
    let target = SettingsTarget::new(mtm);
    let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
    for (i, model) in MeaningModel::ALL.iter().enumerate() {
        popup.addItemWithTitle(&NSString::from_str(title_of(*model)));
        if let Some(item) = popup.lastItem() {
            item.setTag(i as isize);
        }
    }
    if let Some(menu) = popup.menu() {
        menu.addItem(&NSMenuItem::separatorItem(mtm));
    }
    popup.addItemWithTitle(&NSString::from_str(strings::SETTINGS_MEANING_OFF));
    if let Some(item) = popup.lastItem() {
        item.setTag(OFF_TAG);
    }
    // SAFETY: the target lives as long as the window, kept with it.
    unsafe {
        popup.setTarget(Some(&target));
        popup.setAction(Some(sel!(chooseMeaning:)));
    }
    let label = NSTextField::labelWithString(&NSString::from_str(strings::SETTINGS_MEANING), mtm);
    let note = NSTextField::wrappingLabelWithString(&NSString::from_str(strings::SETTINGS_MEANING_NOTE), mtm);
    note.setTextColor(Some(&NSColor::secondaryLabelColor()));
    note.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
    let view = NSView::new(mtm);
    for v in [&*label as &NSView, &popup, &note] {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(v);
    }
    let constraints = [
        view.widthAnchor().constraintEqualToConstant(500.0),
        label.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 24.0),
        label.centerYAnchor().constraintEqualToAnchor(&popup.centerYAnchor()),
        popup.topAnchor().constraintEqualToAnchor_constant(&view.topAnchor(), 24.0),
        popup.leadingAnchor().constraintEqualToAnchor_constant(&label.trailingAnchor(), 8.0),
        popup.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
        note.topAnchor().constraintEqualToAnchor_constant(&popup.bottomAnchor(), 12.0),
        note.leadingAnchor().constraintEqualToAnchor(&popup.leadingAnchor()),
        note.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
        note.bottomAnchor().constraintEqualToAnchor_constant(&view.bottomAnchor(), -24.0),
    ];
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
    let search = NSViewController::new(mtm);
    search.setView(&view);
    let item = NSTabViewItem::tabViewItemWithViewController(&search);
    item.setLabel(&NSString::from_str(strings::SETTINGS_SEARCH));
    item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("magnifyingglass"), None).as_deref());
    let tabs = NSTabViewController::new(mtm);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Toolbar);
    tabs.addTabViewItem(&item);
    let window = NSWindow::windowWithContentViewController(&tabs);
    window.setStyleMask(NSWindowStyleMask::Titled | NSWindowStyleMask::Closable);
    window.setTitle(&NSString::from_str(strings::SETTINGS_SEARCH));
    // SAFETY: the window is kept here; closing it must not release it.
    unsafe { window.setReleasedWhenClosed(false) };
    window.center();
    Settings { window, popup, _tabs: tabs, _target: target }
}

/// Probe: `setting` (a model's key, or `off`) chosen in the menu, as a click would.
pub fn probe_choose(mtm: MainThreadMarker, setting: &str) {
    show(mtm);
    let tag = MeaningModel::ALL.iter().position(|m| m.key() == setting).map_or(OFF_TAG, |i| i as isize);
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            settings.popup.selectItemWithTag(tag);
        }
    });
    chosen(tag);
}

/// Probe: the settings window, when shown.
pub fn probe_window() -> Option<Retained<NSWindow>> {
    SHOWN.with(|shown| shown.borrow().as_ref().map(|s| s.window.clone()))
}

/// Probe: the window's title, its tabs, the choices and the one chosen; nothing when not shown.
pub fn probe_lines() -> Vec<String> {
    SHOWN.with(|shown| {
        let Some(settings) = shown.borrow().as_ref().map(|s| (s.window.clone(), s.popup.clone())) else { return Vec::new() };
        let (window, popup) = settings;
        let choices: Vec<String> = popup.itemTitles().iter().map(|t| t.to_string()).filter(|t| !t.is_empty()).collect();
        let chosen = popup.titleOfSelectedItem().map(|t| t.to_string()).unwrap_or_default();
        vec![format!("settings: {} | meaning: {chosen} | choices: {}", window.title(), choices.join(" / "))]
    })
}
