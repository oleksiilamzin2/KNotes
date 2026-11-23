//! The settings' «Щоденник» tab: the folder the front window's vault makes its daily notes in
//! (the calendar's) and how they are laid out there (all together, or in folders by the year,
//! the month and the week), written into its root's rules file (`"daily": {"folder", "layout"}`).
//! Notes already there stay where they are: a day's note is found by its name wherever it lies.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use knotes_app::{DailyLayout, FolderRules, daily_note_path, day_at, with_daily};
use knotes_core::FolderRulesStore;
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSColor, NSFont, NSLayoutConstraint, NSPopUpButton, NSTextField, NSView};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSRect, NSString};

use crate::strings;

pub struct Ivars {
    vault: RefCell<Option<PathBuf>>,
    title: RefCell<Option<Retained<NSTextField>>>,
    folder: RefCell<Option<Retained<NSTextField>>>,
    /// The layouts, in `DailyLayout::ALL`'s order.
    layout: RefCell<Option<Retained<NSPopUpButton>>>,
    /// Where today's note would be made.
    example: RefCell<Option<Retained<NSTextField>>>,
    relayout: RefCell<Option<Retained<objc2_app_kit::NSButton>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesDailyTab"]
    #[ivars = Ivars]
    pub struct DailyTab;

    unsafe impl NSObjectProtocol for DailyTab {}

    impl DailyTab {
        /// The folder typed (Return, or the field left), or a layout chosen.
        #[unsafe(method(changed:))]
        fn changed(&self, _sender: Option<&AnyObject>) {
            self.write();
        }

        /// «Перекласти наявні нотатки…»: the vault's window moves them, once it has asked.
        #[unsafe(method(relayout:))]
        fn relayout(&self, _sender: Option<&AnyObject>) {
            // What is typed and not yet entered is the folder meant.
            self.write();
            let vault = self.ivars().vault.borrow().clone();
            if let Some(vault) = vault {
                ON_RELAYOUT.with(|callback| {
                    if let Some(callback) = callback.borrow().as_ref() {
                        callback(vault);
                    }
                });
            }
            self.refresh();
        }
    }
);

type OnRelayout = Box<dyn Fn(PathBuf)>;

thread_local! {
    static ON_RELAYOUT: RefCell<Option<OnRelayout>> = const { RefCell::new(None) };
}

/// Runs with the vault whose daily notes the tab's button asks to move into the layout set.
pub fn on_relayout(callback: impl Fn(PathBuf) + 'static) {
    ON_RELAYOUT.with(|c| *c.borrow_mut() = Some(Box::new(callback)));
}

impl DailyTab {
    pub fn make(mtm: MainThreadMarker) -> (Retained<Self>, Retained<NSView>) {
        let this: Retained<Self> = {
            let this = Self::alloc(mtm).set_ivars(Ivars { vault: RefCell::new(None), title: RefCell::new(None), folder: RefCell::new(None), layout: RefCell::new(None), example: RefCell::new(None), relayout: RefCell::new(None) });
            unsafe { msg_send![super(this), init] }
        };
        let view = NSView::new(mtm);
        let title = NSTextField::labelWithString(&NSString::new(), mtm);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize())));
        let folder_label = NSTextField::labelWithString(&NSString::from_str(strings::DAILY_FOLDER_LABEL), mtm);
        let folder = NSTextField::textFieldWithString(&NSString::new(), mtm);
        let layout_label = NSTextField::labelWithString(&NSString::from_str(strings::DAILY_LAYOUT_LABEL), mtm);
        let layout = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        for name in strings::DAILY_LAYOUTS {
            layout.addItemWithTitle(&NSString::from_str(name));
        }
        let example = NSTextField::labelWithString(&NSString::new(), mtm);
        example.setTextColor(Some(&NSColor::secondaryLabelColor()));
        example.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingMiddle);
        // SAFETY: the tab lives as long as the settings window, kept with it.
        let relayout = unsafe { objc2_app_kit::NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::DAILY_RELAYOUT), Some(&this), Some(sel!(relayout:)), mtm) };
        let note = NSTextField::wrappingLabelWithString(&NSString::from_str(strings::DAILY_NOTE), mtm);
        note.setTextColor(Some(&NSColor::secondaryLabelColor()));
        note.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        // SAFETY: the tab lives as long as the settings window, kept with it.
        unsafe {
            folder.setTarget(Some(&this));
            folder.setAction(Some(sel!(changed:)));
            layout.setTarget(Some(&this));
            layout.setAction(Some(sel!(changed:)));
        }
        folder.cell().expect("a text field has a cell").setSendsActionOnEndEditing(true);
        for v in [&*title as &NSView, &folder_label, &folder, &layout_label, &layout, &example, &relayout, &note] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            view.addSubview(v);
        }
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            view.widthAnchor().constraintEqualToConstant(560.0),
            title.topAnchor().constraintEqualToAnchor_constant(&view.topAnchor(), 24.0),
            title.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 24.0),
            title.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            folder.topAnchor().constraintEqualToAnchor_constant(&title.bottomAnchor(), 16.0),
            folder_label.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            folder_label.widthAnchor().constraintEqualToConstant(90.0),
            folder_label.centerYAnchor().constraintEqualToAnchor(&folder.centerYAnchor()),
            folder.leadingAnchor().constraintEqualToAnchor_constant(&folder_label.trailingAnchor(), 8.0),
            folder.widthAnchor().constraintEqualToConstant(260.0),
            layout.topAnchor().constraintEqualToAnchor_constant(&folder.bottomAnchor(), 10.0),
            layout_label.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            layout_label.centerYAnchor().constraintEqualToAnchor(&layout.centerYAnchor()),
            layout.leadingAnchor().constraintEqualToAnchor(&folder.leadingAnchor()),
            layout.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            example.topAnchor().constraintEqualToAnchor_constant(&layout.bottomAnchor(), 10.0),
            example.leadingAnchor().constraintEqualToAnchor(&folder.leadingAnchor()),
            example.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            relayout.topAnchor().constraintEqualToAnchor_constant(&example.bottomAnchor(), 12.0),
            relayout.leadingAnchor().constraintEqualToAnchor(&folder.leadingAnchor()),
            note.topAnchor().constraintEqualToAnchor_constant(&relayout.bottomAnchor(), 14.0),
            note.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            note.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            // Held at the top; the window, as tall as its tallest tab, leaves the room below.
            view.bottomAnchor().constraintGreaterThanOrEqualToAnchor_constant(&note.bottomAnchor(), 24.0),
        ]));
        *this.ivars().title.borrow_mut() = Some(title);
        *this.ivars().folder.borrow_mut() = Some(folder);
        *this.ivars().layout.borrow_mut() = Some(layout);
        *this.ivars().example.borrow_mut() = Some(example);
        *this.ivars().relayout.borrow_mut() = Some(relayout);
        (this, view)
    }

    /// The vault whose daily notes are set (the front window's; none: no vault open).
    pub fn set_vault(&self, vault: Option<PathBuf>) {
        *self.ivars().vault.borrow_mut() = vault;
        self.refresh();
    }

    /// The folder and the layout as the vault's root's rules file says now.
    pub fn refresh(&self) {
        let vault = self.ivars().vault.borrow().clone();
        let rules = self.root();
        let layout = rules.as_ref().map(FolderRules::daily_layout).unwrap_or_default();
        if let Some(title) = self.ivars().title.borrow().as_ref() {
            let text = match &vault {
                Some(vault) => strings::daily_of(&vault.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                None => strings::SPACES_NO_VAULT.to_owned(),
            };
            title.setStringValue(&NSString::from_str(&text));
        }
        if let Some(field) = self.ivars().folder.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(rules.as_ref().and_then(FolderRules::daily_folder).unwrap_or(strings::DAILY_FOLDER)));
            field.setEnabled(vault.is_some());
        }
        if let Some(popup) = self.ivars().layout.borrow().as_ref() {
            popup.selectItemAtIndex(DailyLayout::ALL.iter().position(|l| *l == layout).unwrap_or(0) as isize);
            popup.setEnabled(vault.is_some());
        }
        if let Some(button) = self.ivars().relayout.borrow().as_ref() {
            button.setEnabled(vault.is_some());
        }
        if let Some(example) = self.ivars().example.borrow().as_ref() {
            // Today's note, where it would be made.
            let offset = objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT() as i32;
            let by_folder: HashMap<String, FolderRules> = rules.map(|r| (String::new(), r)).into_iter().collect();
            let path = daily_note_path(&by_folder, strings::DAILY_FOLDER, day_at(knotes_session::now(), offset));
            example.setStringValue(&NSString::from_str(&if vault.is_some() { strings::daily_example(&path) } else { String::new() }));
        }
    }

    /// The root's rules, parsed; empty ones when it has no file.
    fn root(&self) -> Option<FolderRules> {
        let vault = self.ivars().vault.borrow().clone()?;
        let text = FsVault::new(&vault).read_rules(&[""]).ok().and_then(|r| r.into_iter().next()).map_or_else(|| "{}".to_owned(), |(_, text)| text);
        FolderRules::parse(&text).ok()
    }

    /// Writes the folder typed and the layout chosen into the root's rules file, its other keys
    /// kept.
    fn write(&self) {
        let Some(vault) = self.ivars().vault.borrow().clone() else { return };
        let folder = self.ivars().folder.borrow().as_ref().map(|f| f.stringValue().to_string()).unwrap_or_default();
        let layout = self.ivars().layout.borrow().as_ref().and_then(|p| DailyLayout::ALL.get(usize::try_from(p.indexOfSelectedItem()).ok()?).copied()).unwrap_or_default();
        let files = FsVault::new(&vault);
        // The file as it is: one that cannot be read or does not parse is left, and said.
        let written = files
            .read_rules(&[""])
            .map_err(|e| e.to_string())
            .and_then(|found| with_daily(&found.into_iter().next().map(|(_, text)| text).unwrap_or_default(), folder.trim(), layout))
            .and_then(|text| files.write_rules("", &text).map_err(|e| e.to_string()));
        if let Err(e) = written {
            crate::rename_dialog::tell_failure(self.mtm(), None, strings::DAILY_FAILED, &e);
        }
        self.refresh();
    }

    /// The tab for the probe: the vault, its folder, its layout and where today's note goes.
    pub fn probe_line(&self) -> String {
        let vault = self.ivars().vault.borrow().clone().and_then(|v| v.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
        let folder = self.ivars().folder.borrow().as_ref().map(|f| f.stringValue().to_string()).unwrap_or_default();
        let layout = self.ivars().layout.borrow().as_ref().and_then(|p| p.titleOfSelectedItem()).map(|t| t.to_string()).unwrap_or_default();
        let example = self.ivars().example.borrow().as_ref().map(|f| f.stringValue().to_string()).unwrap_or_default();
        format!("{vault} | {folder} | {layout} | {example}")
    }

    /// `folder=Journal` typed into the field, `layout=week` chosen in the menu, or `relayout`:
    /// the button clicked, as the user does it (probe mode).
    pub fn probe_set(&self, spec: &str) {
        if spec == "relayout" {
            let button = self.ivars().relayout.borrow().clone();
            if let Some(button) = button {
                // SAFETY: a click on the tab's own button.
                unsafe { button.performClick(None) };
            }
            return;
        }
        let Some((what, value)) = spec.split_once('=') else { return };
        if what == "folder" {
            if let Some(field) = self.ivars().folder.borrow().as_ref() {
                field.setStringValue(&NSString::from_str(value));
            }
        } else if let (Some(popup), Some(at)) = (self.ivars().layout.borrow().as_ref(), DailyLayout::from_key(value).and_then(|l| DailyLayout::ALL.iter().position(|a| *a == l))) {
            popup.selectItemAtIndex(at as isize);
        }
        self.write();
    }
}
