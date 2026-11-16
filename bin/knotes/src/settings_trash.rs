//! The settings' «Видалені» tab (ADR 0032): how many days the front window's vault keeps what is
//! deleted, written into its root's rules file (`"trash": {"days"}`).

use std::cell::RefCell;
use std::path::PathBuf;

use knotes_app::{DEFAULT_TRASH_DAYS, FolderRules};
use knotes_core::FolderRulesStore;
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSColor, NSFont, NSLayoutConstraint, NSStepper, NSTextField, NSView};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};

use crate::strings;

/// The most days a vault may keep deleted things.
const MOST_DAYS: u32 = 3650;

pub struct Ivars {
    vault: RefCell<Option<PathBuf>>,
    title: RefCell<Option<Retained<NSTextField>>>,
    days: RefCell<Option<Retained<NSTextField>>>,
    stepper: RefCell<Option<Retained<NSStepper>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesTrashTab"]
    #[ivars = Ivars]
    pub struct TrashTab;

    unsafe impl NSObjectProtocol for TrashTab {}

    impl TrashTab {
        /// The days typed (Return, or the field left): written when a number of them.
        #[unsafe(method(daysTyped:))]
        fn days_typed(&self, sender: &NSTextField) {
            match sender.stringValue().to_string().trim().parse::<u32>() {
                Ok(days) => self.write(days),
                Err(_) => self.refresh(),
            }
        }

        /// The stepper's arrows.
        #[unsafe(method(daysStepped:))]
        fn days_stepped(&self, sender: &NSStepper) {
            self.write(sender.integerValue().max(1) as u32);
        }
    }
);

impl TrashTab {
    pub fn make(mtm: MainThreadMarker) -> (Retained<Self>, Retained<NSView>) {
        let this: Retained<Self> = {
            let this = Self::alloc(mtm).set_ivars(Ivars { vault: RefCell::new(None), title: RefCell::new(None), days: RefCell::new(None), stepper: RefCell::new(None) });
            unsafe { msg_send![super(this), init] }
        };
        let view = NSView::new(mtm);
        let title = NSTextField::labelWithString(&NSString::new(), mtm);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize())));
        let keep = NSTextField::labelWithString(&NSString::from_str(strings::TRASH_KEEP), mtm);
        let days = NSTextField::textFieldWithString(&NSString::new(), mtm);
        days.widthAnchor().constraintEqualToConstant(64.0).setActive(true);
        let stepper = NSStepper::new(mtm);
        stepper.setMinValue(1.0);
        stepper.setMaxValue(f64::from(MOST_DAYS));
        stepper.setIncrement(1.0);
        stepper.setValueWraps(false);
        let unit = NSTextField::labelWithString(&NSString::from_str(strings::TRASH_DAYS), mtm);
        let note = NSTextField::wrappingLabelWithString(&NSString::from_str(strings::TRASH_NOTE), mtm);
        note.setTextColor(Some(&NSColor::secondaryLabelColor()));
        note.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        // SAFETY: the tab lives as long as the settings window, kept with it.
        unsafe {
            days.setTarget(Some(&this));
            days.setAction(Some(sel!(daysTyped:)));
            stepper.setTarget(Some(&this));
            stepper.setAction(Some(sel!(daysStepped:)));
        }
        days.cell().expect("a text field has a cell").setSendsActionOnEndEditing(true);
        for v in [&*title as &NSView, &keep, &days, &stepper, &unit, &note] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            view.addSubview(v);
        }
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            view.widthAnchor().constraintEqualToConstant(560.0),
            title.topAnchor().constraintEqualToAnchor_constant(&view.topAnchor(), 24.0),
            title.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 24.0),
            title.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            days.topAnchor().constraintEqualToAnchor_constant(&title.bottomAnchor(), 16.0),
            keep.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            keep.centerYAnchor().constraintEqualToAnchor(&days.centerYAnchor()),
            days.leadingAnchor().constraintEqualToAnchor_constant(&keep.trailingAnchor(), 8.0),
            stepper.leadingAnchor().constraintEqualToAnchor_constant(&days.trailingAnchor(), 4.0),
            stepper.centerYAnchor().constraintEqualToAnchor(&days.centerYAnchor()),
            unit.leadingAnchor().constraintEqualToAnchor_constant(&stepper.trailingAnchor(), 6.0),
            unit.centerYAnchor().constraintEqualToAnchor(&days.centerYAnchor()),
            note.topAnchor().constraintEqualToAnchor_constant(&days.bottomAnchor(), 12.0),
            note.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            note.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            // Held at the top; the window, as tall as its tallest tab, leaves the room below.
            view.bottomAnchor().constraintGreaterThanOrEqualToAnchor_constant(&note.bottomAnchor(), 24.0),
        ]));
        *this.ivars().title.borrow_mut() = Some(title);
        *this.ivars().days.borrow_mut() = Some(days);
        *this.ivars().stepper.borrow_mut() = Some(stepper);
        (this, view)
    }

    /// The vault whose days are shown (the front window's; none: no vault open).
    pub fn set_vault(&self, vault: Option<PathBuf>) {
        *self.ivars().vault.borrow_mut() = vault;
        self.refresh();
    }

    /// The vault's days as its root's rules file says now.
    pub fn refresh(&self) {
        let vault = self.ivars().vault.borrow().clone();
        let days = self.root().map_or(DEFAULT_TRASH_DAYS, |rules| rules.trash_days());
        if let Some(title) = self.ivars().title.borrow().as_ref() {
            let text = match &vault {
                Some(vault) => strings::trash_of(&vault.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                None => strings::SPACES_NO_VAULT.to_owned(),
            };
            title.setStringValue(&NSString::from_str(&text));
        }
        if let Some(field) = self.ivars().days.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(&days.to_string()));
            field.setEnabled(vault.is_some());
        }
        if let Some(stepper) = self.ivars().stepper.borrow().as_ref() {
            stepper.setIntegerValue(days as isize);
            stepper.setEnabled(vault.is_some());
        }
    }

    /// The root's rules, parsed; empty ones when it has no file.
    fn root(&self) -> Option<FolderRules> {
        let vault = self.ivars().vault.borrow().clone()?;
        let text = FsVault::new(&vault).read_rules(&[""]).ok().and_then(|r| r.into_iter().next()).map_or_else(|| "{}".to_owned(), |(_, text)| text);
        FolderRules::parse(&text).ok()
    }

    /// Writes `days` (a day to ten years) into the root's rules file, its other keys kept.
    fn write(&self, days: u32) {
        let Some(vault) = self.ivars().vault.borrow().clone() else { return };
        let Some(mut rules) = self.root() else { return };
        rules.set_trash_days(days.clamp(1, MOST_DAYS));
        if let Err(e) = FsVault::new(&vault).write_rules("", &rules.to_json()) {
            eprintln!("knotes: cannot write the days deleted things are kept: {e}");
        }
        self.refresh();
    }

    /// The tab for the probe: the vault and its days.
    pub fn probe_line(&self) -> String {
        let vault = self.ivars().vault.borrow().clone().and_then(|v| v.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
        let days = self.ivars().days.borrow().as_ref().map(|f| f.stringValue().to_string()).unwrap_or_default();
        format!("{vault} | {days}")
    }

    /// Days typed into the field, as the user types them (probe mode).
    pub fn probe_type(&self, days: &str) {
        let Some(field) = self.ivars().days.borrow().clone() else { return };
        field.setStringValue(&NSString::from_str(days));
        let target: &AnyObject = self.as_ref();
        // SAFETY: the field's own action, sent to the tab as the field does.
        let _: () = unsafe { msg_send![target, daysTyped: &*field] };
    }
}
