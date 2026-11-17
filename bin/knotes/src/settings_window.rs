//! The settings window (Cmd+,), a tab a subject (ADR 0029): «Пошук» chooses the model search by
//! meaning runs on, or turns search by meaning off; «Простори» edits the front window's vault's
//! spaces (ADR 0030).

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime};

use knotes_vectors::MeaningModel;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSButton, NSColor, NSControlSize, NSFont, NSImage, NSLayoutConstraint, NSMenuItem, NSPopUpButton, NSProgressIndicator, NSProgressIndicatorStyle, NSTabViewController, NSTabViewControllerTabStyle, NSTabViewItem,
    NSTextAlignment, NSTextField, NSView, NSViewController, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSRect, NSString};

use crate::settings_spaces::SpacesTab;
use crate::settings_trash::TrashTab;
use crate::strings;

/// The tag of the item turning search by meaning off (a model's is its place in
/// `MeaningModel::ALL`).
const OFF_TAG: isize = 99;

type OnMeaning = Box<dyn Fn()>;

/// A model's row of stored vectors: their size, the button clearing them (a model not in use) and
/// the one making them again (the model in use).
struct CacheRow {
    model: MeaningModel,
    size: Retained<NSTextField>,
    clear: Retained<NSButton>,
    regenerate: Retained<NSButton>,
}

struct Settings {
    window: Retained<NSWindow>,
    popup: Retained<NSPopUpButton>,
    caches: Vec<CacheRow>,
    /// The vectors being made: how far, how fast.
    status: Retained<NSTextField>,
    progress: Retained<NSProgressIndicator>,
    spaces: Retained<SpacesTab>,
    trash: Retained<TrashTab>,
    tabs: Retained<NSTabViewController>,
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

        #[unsafe(method(clearCache:))]
        fn clear_cache(&self, sender: &NSButton) {
            if let Some(model) = usize::try_from(sender.tag()).ok().and_then(|i| MeaningModel::ALL.get(i).copied()) {
                cleared(model);
            }
        }

        #[unsafe(method(regenerateCache:))]
        fn regenerate_cache(&self, sender: &NSButton) {
            if let Some(model) = usize::try_from(sender.tag()).ok().and_then(|i| MeaningModel::ALL.get(i).copied()) {
                cleared(model);
            }
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

/// Shows the window (made at the first call) with the current choices; its «Простори» tab
/// shows `vault`'s spaces (the front window's).
pub fn show(mtm: MainThreadMarker, vault: Option<PathBuf>) {
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        let settings = shown.get_or_insert_with(|| make(mtm));
        settings.spaces.set_vault(vault.clone());
        settings.trash.set_vault(vault);
        let tag = crate::meaning::model().and_then(|m| MeaningModel::ALL.iter().position(|a| *a == m)).map_or(OFF_TAG, |i| i as isize);
        settings.popup.selectItemWithTag(tag);
        refresh(settings);
        settings.window.makeKeyAndOrderFront(None);
    });
    tick();
}

/// Shows the window on its «Простори» tab (the space switcher's «Налаштувати простори…»).
pub fn show_spaces(mtm: MainThreadMarker, vault: Option<PathBuf>) {
    show(mtm, vault);
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            settings.tabs.setSelectedTabViewItemIndex(1);
            settings.window.setTitle(&NSString::from_str(strings::SETTINGS_SPACES));
        }
    });
}

/// Shows the window on its «Видалені» tab.
pub fn show_trash(mtm: MainThreadMarker, vault: Option<PathBuf>) {
    show(mtm, vault);
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            settings.tabs.setSelectedTabViewItemIndex(2);
            settings.window.setTitle(&NSString::from_str(strings::SETTINGS_TRASH));
        }
    });
}

/// Probe: days typed into the «Видалені» tab's field.
pub fn probe_trash_days(days: &str) {
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            settings.trash.probe_type(days);
        }
    });
}

/// While the window is shown, its sizes and progress are looked at again twice a second.
fn tick() {
    let shown = SHOWN.with(|shown| shown.borrow().as_ref().is_some_and(|s| s.window.isVisible()));
    if !shown {
        return;
    }
    let when = DispatchTime::try_from(Duration::from_millis(500)).expect("short delay");
    let _ = DispatchQueue::main().after(when, || {
        SHOWN.with(|shown| {
            if let Some(settings) = shown.borrow().as_ref() {
                refresh(settings);
                settings.spaces.refresh();
            }
        });
        tick();
    });
}

/// Each model's stored vectors as they are on disk now, and those being made.
fn refresh(settings: &Settings) {
    let in_use = crate::meaning::model();
    for CacheRow { model, size, clear, regenerate } in &settings.caches {
        let bytes = crate::meaning::cache_size(*model);
        size.setStringValue(&NSString::from_str(&if bytes == 0 { strings::SETTINGS_CACHE_NONE.to_string() } else { strings::file_size(bytes) }));
        // The model in use is made again rather than cleared (cleared, it would be anyway): its
        // row shows «Перегенерувати», the others «Очистити».
        let used = in_use == Some(*model);
        clear.setHidden(used);
        clear.setEnabled(bytes > 0);
        regenerate.setHidden(!used);
    }
    match (in_use, crate::meaning::making()) {
        // A pass under way works; one that failed said so in its place.
        (Some(_), None) if let Some(why) = crate::meaning::failing() => {
            settings.status.setStringValue(&NSString::from_str(&strings::settings_meaning_failing(&why)));
            settings.progress.setHidden(true);
        }
        (None, _) => {
            settings.status.setStringValue(&NSString::from_str(strings::SETTINGS_MEANING_IS_OFF));
            settings.progress.setHidden(true);
        }
        (Some(_), Some(making)) => {
            settings.status.setStringValue(&NSString::from_str(&strings::settings_making(making.done, making.total, making.per_second)));
            settings.progress.setMaxValue(making.total.max(1) as f64);
            settings.progress.setDoubleValue(making.done as f64);
            settings.progress.setHidden(false);
        }
        (Some(_), None) => {
            settings.status.setStringValue(&NSString::from_str(strings::SETTINGS_VECTORS_READY));
            settings.progress.setHidden(true);
        }
    }
}

/// `model`'s stored vectors removed; the windows open fresh stores when it is the one in use
/// (its vectors are embedded again in the background: «Перегенерувати»).
fn cleared(model: MeaningModel) {
    crate::meaning::clear_cache(model);
    if crate::meaning::model() == Some(model) {
        ON_MEANING.with(|c| {
            if let Some(callback) = c.borrow().as_ref() {
                callback();
            }
        });
    }
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            refresh(settings);
        }
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
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            refresh(settings);
        }
    });
}

fn short_title_of(model: MeaningModel) -> &'static str {
    match model {
        MeaningModel::E5Large => strings::SETTINGS_CACHE_E5,
        MeaningModel::EmbeddingGemma2 => strings::SETTINGS_CACHE_GEMMA,
    }
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
    let mut constraints = vec![
        view.widthAnchor().constraintEqualToConstant(560.0),
        label.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 24.0),
        label.centerYAnchor().constraintEqualToAnchor(&popup.centerYAnchor()),
        popup.topAnchor().constraintEqualToAnchor_constant(&view.topAnchor(), 24.0),
        popup.leadingAnchor().constraintEqualToAnchor_constant(&label.trailingAnchor(), 8.0),
        popup.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
        note.topAnchor().constraintEqualToAnchor_constant(&popup.bottomAnchor(), 12.0),
        note.leadingAnchor().constraintEqualToAnchor(&popup.leadingAnchor()),
        note.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
    ];
    // Each model's stored vectors: a row of its name, their size and a button clearing them.
    let cache_label = NSTextField::labelWithString(&NSString::from_str(strings::SETTINGS_CACHE), mtm);
    cache_label.setTranslatesAutoresizingMaskIntoConstraints(false);
    view.addSubview(&cache_label);
    let mut caches = Vec::new();
    let mut above = note.bottomAnchor();
    let mut gap = 20.0;
    for (i, model) in MeaningModel::ALL.iter().enumerate() {
        let name = NSTextField::labelWithString(&NSString::from_str(short_title_of(*model)), mtm);
        let size = NSTextField::labelWithString(&NSString::new(), mtm);
        size.setAlignment(NSTextAlignment::Right);
        size.setTextColor(Some(&NSColor::secondaryLabelColor()));
        // SAFETY: the target lives as long as the window, kept with it.
        let clear = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::SETTINGS_CLEAR), Some(&target), Some(sel!(clearCache:)), mtm) };
        clear.setTag(i as isize);
        clear.setControlSize(NSControlSize::Small);
        // SAFETY: as the button above.
        let regenerate = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::SETTINGS_REGENERATE), Some(&target), Some(sel!(regenerateCache:)), mtm) };
        regenerate.setTag(i as isize);
        regenerate.setControlSize(NSControlSize::Small);
        for v in [&*name as &NSView, &size, &clear, &regenerate] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            view.addSubview(v);
        }
        constraints.extend([
            clear.topAnchor().constraintEqualToAnchor_constant(&above, gap),
            name.leadingAnchor().constraintEqualToAnchor(&popup.leadingAnchor()),
            name.widthAnchor().constraintEqualToConstant(150.0),
            name.centerYAnchor().constraintEqualToAnchor(&clear.centerYAnchor()),
            size.leadingAnchor().constraintEqualToAnchor_constant(&name.trailingAnchor(), 8.0),
            size.widthAnchor().constraintEqualToConstant(70.0),
            size.centerYAnchor().constraintEqualToAnchor(&clear.centerYAnchor()),
            clear.leadingAnchor().constraintEqualToAnchor_constant(&size.trailingAnchor(), 12.0),
            // In the same place: a row shows the one that applies.
            regenerate.leadingAnchor().constraintEqualToAnchor(&clear.leadingAnchor()),
            regenerate.centerYAnchor().constraintEqualToAnchor(&clear.centerYAnchor()),
        ]);
        if i == 0 {
            constraints.push(cache_label.trailingAnchor().constraintEqualToAnchor(&label.trailingAnchor()));
            constraints.push(cache_label.centerYAnchor().constraintEqualToAnchor(&clear.centerYAnchor()));
        }
        above = clear.bottomAnchor();
        gap = 6.0;
        caches.push(CacheRow { model: *model, size, clear, regenerate });
    }
    // The vectors being made: a line saying how far and how fast, a bar under it.
    let status = NSTextField::labelWithString(&NSString::new(), mtm);
    status.setTextColor(Some(&NSColor::secondaryLabelColor()));
    status.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
    let progress = NSProgressIndicator::new(mtm);
    progress.setStyle(NSProgressIndicatorStyle::Bar);
    progress.setIndeterminate(false);
    progress.setHidden(true);
    for v in [&*status as &NSView, &progress] {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(v);
    }
    constraints.extend([
        status.topAnchor().constraintEqualToAnchor_constant(&above, 16.0),
        status.leadingAnchor().constraintEqualToAnchor(&popup.leadingAnchor()),
        status.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
        progress.topAnchor().constraintEqualToAnchor_constant(&status.bottomAnchor(), 6.0),
        progress.leadingAnchor().constraintEqualToAnchor(&popup.leadingAnchor()),
        progress.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
        view.bottomAnchor().constraintEqualToAnchor_constant(&progress.bottomAnchor(), 24.0),
    ]);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
    let search = NSViewController::new(mtm);
    search.setView(&view);
    let item = NSTabViewItem::tabViewItemWithViewController(&search);
    item.setLabel(&NSString::from_str(strings::SETTINGS_SEARCH));
    item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("magnifyingglass"), None).as_deref());
    let tabs = NSTabViewController::new(mtm);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Toolbar);
    tabs.addTabViewItem(&item);
    let (spaces, spaces_view) = SpacesTab::make(mtm);
    let spaces_vc = NSViewController::new(mtm);
    spaces_vc.setView(&spaces_view);
    let spaces_item = NSTabViewItem::tabViewItemWithViewController(&spaces_vc);
    spaces_item.setLabel(&NSString::from_str(strings::SETTINGS_SPACES));
    spaces_item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("square.stack"), None).as_deref());
    tabs.addTabViewItem(&spaces_item);
    let (trash, trash_view) = TrashTab::make(mtm);
    let trash_vc = NSViewController::new(mtm);
    trash_vc.setView(&trash_view);
    let trash_item = NSTabViewItem::tabViewItemWithViewController(&trash_vc);
    trash_item.setLabel(&NSString::from_str(strings::SETTINGS_TRASH));
    trash_item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("trash"), None).as_deref());
    tabs.addTabViewItem(&trash_item);
    let window = NSWindow::windowWithContentViewController(&tabs);
    window.setStyleMask(NSWindowStyleMask::Titled | NSWindowStyleMask::Closable);
    window.setTitle(&NSString::from_str(strings::SETTINGS_SEARCH));
    // SAFETY: the window is kept here; closing it must not release it.
    unsafe { window.setReleasedWhenClosed(false) };
    window.center();
    crate::probe::conceal(&window);
    Settings { window, popup, caches, status, progress, spaces, trash, tabs, _target: target }
}

/// Probe: an edit made through the «Простори» tab's controls (see `SpacesTab::probe_edit`).
pub fn probe_spaces_edit(spec: &str) {
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            settings.spaces.probe_edit(spec);
        }
    });
}

/// Probe: `setting` (a model's key, or `off`) chosen in the menu, as a click would.
pub fn probe_choose(mtm: MainThreadMarker, setting: &str) {
    show(mtm, None);
    let tag = MeaningModel::ALL.iter().position(|m| m.key() == setting).map_or(OFF_TAG, |i| i as isize);
    SHOWN.with(|shown| {
        if let Some(settings) = shown.borrow().as_ref() {
            settings.popup.selectItemWithTag(tag);
        }
    });
    chosen(tag);
}

/// Probe: `model`'s stored vectors cleared, as a click on its button would.
pub fn probe_clear(mtm: MainThreadMarker, key: &str) {
    show(mtm, None);
    if let Some(model) = MeaningModel::from_key(key) {
        cleared(model);
    }
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
        let caches: Vec<String> = shown
            .borrow()
            .as_ref()
            .map(|s| {
                s.caches
                    .iter()
                    .map(|CacheRow { model, size, clear, regenerate }| {
                        let clear = if !clear.isHidden() && clear.isEnabled() { " [clear]" } else { "" };
                        let regenerate = if regenerate.isHidden() { "" } else { " [regenerate]" };
                        format!("{} {}{clear}{regenerate}", model.key(), size.stringValue())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let status = shown.borrow().as_ref().map(|s| s.status.stringValue().to_string()).unwrap_or_default();
        let spaces = shown.borrow().as_ref().map(|s| s.spaces.probe_line()).unwrap_or_default();
        vec![
            format!("settings: {} | meaning: {chosen} | choices: {}", window.title(), choices.join(" / ")),
            format!("settings-cache: {}", caches.join(" | ")),
            format!("settings-progress: {status}"),
            format!("settings-spaces: {spaces}"),
            format!("settings-space-colors: {}", shown.borrow().as_ref().map(|s| s.spaces.probe_colors()).unwrap_or_default()),
            format!("settings-trash: {}", shown.borrow().as_ref().map(|s| s.trash.probe_line()).unwrap_or_default()),
        ]
    })
}
