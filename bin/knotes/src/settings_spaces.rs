//! The settings' «Простори» tab (ADR 0030): the front window's vault's spaces, a row each of its
//! colour, its name and a button removing it, and a button adding one. Edits write the root's
//! rules file; the windows follow through their snapshots, and the tab reads the file again
//! while shown, so spaces made elsewhere show too.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use knotes_core::FolderRulesStore;
use knotes_spaces::{Space, SpaceColor, Spaces, set_spaces};
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSButton, NSColor, NSColorPanel, NSColorSpace, NSFont, NSLayoutConstraint, NSMenuItem, NSPopUpButton, NSStackView, NSTextField, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSRect, NSString};

use crate::{probe, strings};

/// A row's colour menu: its own colour (shown first when it has one), and the item opening the
/// colour panel; a named colour's tag is its place in `SpaceColor::ALL`.
const OWN_TAG: isize = 100;
const CHOOSE_TAG: isize = 101;

pub struct Ivars {
    /// The vault whose spaces are shown, and the spaces as last shown.
    vault: RefCell<Option<PathBuf>>,
    shown: RefCell<Option<Vec<Space>>>,
    title: RefCell<Option<Retained<NSTextField>>>,
    rows: RefCell<Option<Retained<NSStackView>>>,
    add: RefCell<Option<Retained<NSButton>>>,
    /// The row whose colour the colour panel chooses, while it is open for it.
    picking: Cell<Option<isize>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesSpacesTab"]
    #[ivars = Ivars]
    pub struct SpacesTab;

    unsafe impl NSObjectProtocol for SpacesTab {}

    impl SpacesTab {
        /// A row's name edited (Return, or the field left).
        #[unsafe(method(renameSpace:))]
        fn rename(&self, sender: &NSTextField) {
            let name = sender.stringValue().to_string();
            let name = name.trim();
            self.edit(sender.tag(), |list, i| {
                if !name.is_empty() {
                    list[i].name = name.to_owned();
                }
            });
        }

        /// A row's colour chosen: a named one, or «Свій колір…», which opens the colour panel.
        #[unsafe(method(colorSpace:))]
        fn color(&self, sender: &NSPopUpButton) {
            match sender.selectedTag() {
                OWN_TAG => {}
                CHOOSE_TAG => self.choose_own(sender.tag()),
                tag => {
                    let color = usize::try_from(tag).ok().and_then(|i| SpaceColor::ALL.get(i).copied());
                    self.edit(sender.tag(), |list, i| {
                        if let Some(color) = color {
                            list[i].color = color;
                        }
                    });
                }
            }
        }

        /// The colour panel's colour, for the row it was opened for (sent when the mouse is let
        /// go, not at every move).
        #[unsafe(method(colorPicked:))]
        fn color_picked(&self, sender: &NSColorPanel) {
            let Some(row) = self.ivars().picking.get() else { return };
            let Some(srgb) = sender.color().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) else { return };
            let color = SpaceColor::from_srgb(srgb.redComponent(), srgb.greenComponent(), srgb.blueComponent());
            self.edit(row, |list, i| list[i].color = color);
        }

        /// A row's button: the space goes, once confirmed; its folders stay, out of any space.
        #[unsafe(method(deleteSpace:))]
        fn delete(&self, sender: &NSButton) {
            let row = sender.tag();
            let Some(name) = self.spaces().and_then(|list| usize::try_from(row).ok().and_then(|i| list.get(i).map(|s| s.name.clone()))) else { return };
            if !probe::enabled() {
                let alert = NSAlert::new(self.mtm());
                alert.setMessageText(&NSString::from_str(&strings::delete_space_question(&name)));
                alert.setInformativeText(&NSString::from_str(strings::DELETE_SPACE_NOTE));
                for title in [strings::DELETE, strings::CANCEL] {
                    alert.addButtonWithTitle(&NSString::from_str(title));
                }
                if alert.runModal() != NSAlertFirstButtonReturn {
                    return;
                }
            }
            self.edit(row, |list, i| {
                list.remove(i);
            });
        }

        /// A new space at the end: named «Новий простір», the first colour no space has.
        #[unsafe(method(addSpace:))]
        fn add(&self, _sender: Option<&AnyObject>) {
            let Some(mut list) = self.spaces() else { return };
            list.push(knotes_spaces::new_space(&list, strings::NEW_SPACE_NAME));
            self.write(&list);
        }
    }
);

impl SpacesTab {
    /// The tab's view and its controller.
    pub fn make(mtm: MainThreadMarker) -> (Retained<Self>, Retained<NSView>) {
        let this: Retained<Self> = {
            let this = Self::alloc(mtm).set_ivars(Ivars {
                vault: RefCell::new(None),
                shown: RefCell::new(None),
                title: RefCell::new(None),
                rows: RefCell::new(None),
                add: RefCell::new(None),
                picking: Cell::new(None),
            });
            unsafe { msg_send![super(this), init] }
        };
        let view = NSView::new(mtm);
        let title = NSTextField::labelWithString(&NSString::new(), mtm);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize())));
        let note = NSTextField::wrappingLabelWithString(&NSString::from_str(strings::SPACES_NOTE), mtm);
        note.setTextColor(Some(&NSColor::secondaryLabelColor()));
        note.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        let rows = NSStackView::new(mtm);
        rows.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        rows.setAlignment(objc2_app_kit::NSLayoutAttribute::Leading);
        rows.setSpacing(8.0);
        // SAFETY: the tab lives as long as the settings window, kept with it.
        let add = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::ADD_SPACE), Some(&this), Some(sel!(addSpace:)), mtm) };
        for v in [&*title as &NSView, &note, &rows, &add] {
            v.setTranslatesAutoresizingMaskIntoConstraints(false);
            view.addSubview(v);
        }
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            view.widthAnchor().constraintEqualToConstant(560.0),
            title.topAnchor().constraintEqualToAnchor_constant(&view.topAnchor(), 24.0),
            title.leadingAnchor().constraintEqualToAnchor_constant(&view.leadingAnchor(), 24.0),
            title.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            note.topAnchor().constraintEqualToAnchor_constant(&title.bottomAnchor(), 6.0),
            note.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            note.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            rows.topAnchor().constraintEqualToAnchor_constant(&note.bottomAnchor(), 16.0),
            rows.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            rows.trailingAnchor().constraintEqualToAnchor_constant(&view.trailingAnchor(), -24.0),
            add.topAnchor().constraintEqualToAnchor_constant(&rows.bottomAnchor(), 14.0),
            add.leadingAnchor().constraintEqualToAnchor(&title.leadingAnchor()),
            // Held at the top; the window, as tall as its tallest tab, leaves the room below.
            view.bottomAnchor().constraintGreaterThanOrEqualToAnchor_constant(&add.bottomAnchor(), 24.0),
        ]));
        *this.ivars().title.borrow_mut() = Some(title);
        *this.ivars().rows.borrow_mut() = Some(rows);
        *this.ivars().add.borrow_mut() = Some(add);
        (this, view)
    }

    /// The vault to show the spaces of (the front window's; none: no vault open).
    pub fn set_vault(&self, vault: Option<PathBuf>) {
        if *self.ivars().vault.borrow() != vault {
            *self.ivars().vault.borrow_mut() = vault;
            *self.ivars().shown.borrow_mut() = None;
        }
        self.refresh();
    }

    /// The rows again when the vault's spaces changed, unless a name is being typed.
    pub fn refresh(&self) {
        let spaces = self.spaces();
        if *self.ivars().shown.borrow() == spaces && spaces.is_some() || self.editing() {
            return;
        }
        let vault = self.ivars().vault.borrow().clone();
        if let Some(title) = self.ivars().title.borrow().as_ref() {
            let text = match &vault {
                Some(vault) => strings::spaces_of(&vault_name(vault)),
                None => strings::SPACES_NO_VAULT.to_owned(),
            };
            title.setStringValue(&NSString::from_str(&text));
        }
        if let Some(add) = self.ivars().add.borrow().as_ref() {
            add.setEnabled(vault.is_some());
        }
        let Some(rows) = self.ivars().rows.borrow().clone() else { return };
        for view in rows.arrangedSubviews().iter() {
            rows.removeArrangedSubview(&view);
            view.removeFromSuperview();
        }
        for (i, space) in spaces.iter().flatten().enumerate() {
            rows.addArrangedSubview(&self.row(i, space));
        }
        *self.ivars().shown.borrow_mut() = spaces;
    }

    /// A space's row: its colour, its name, the button removing it.
    fn row(&self, i: usize, space: &Space) -> Retained<NSStackView> {
        let mtm = self.mtm();
        let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        let entry = |title: &str, tag: isize, color: Option<SpaceColor>| {
            popup.addItemWithTitle(&NSString::from_str(title));
            if let Some(item) = popup.lastItem() {
                item.setTag(tag);
                item.setImage(color.and_then(|c| crate::space_ui::dot(c, 11.0, title)).as_deref());
            }
        };
        // One's own colour first, when the space has one.
        if let SpaceColor::Custom(..) = space.color {
            entry(strings::OWN_COLOR, OWN_TAG, Some(space.color));
        }
        for (c, color) in SpaceColor::ALL.iter().enumerate() {
            entry(strings::color_name(*color), c as isize, Some(*color));
        }
        if let Some(menu) = popup.menu() {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        entry(strings::CHOOSE_COLOR, CHOOSE_TAG, None);
        popup.selectItemWithTag(match space.color {
            SpaceColor::Custom(..) => OWN_TAG,
            named => SpaceColor::ALL.iter().position(|c| *c == named).unwrap_or(0) as isize,
        });
        popup.setTag(i as isize);
        let name = NSTextField::textFieldWithString(&NSString::from_str(&space.name), mtm);
        name.setTag(i as isize);
        name.widthAnchor().constraintEqualToConstant(240.0).setActive(true);
        // SAFETY: the tab lives as long as the settings window, kept with it.
        let delete = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::DELETE), Some(self), Some(sel!(deleteSpace:)), mtm) };
        delete.setTag(i as isize);
        // SAFETY: as above.
        unsafe {
            popup.setTarget(Some(self));
            popup.setAction(Some(sel!(colorSpace:)));
            name.setTarget(Some(self));
            name.setAction(Some(sel!(renameSpace:)));
        }
        name.cell().expect("a text field has a cell").setSendsActionOnEndEditing(true);
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setSpacing(10.0);
        for view in [&*popup as &NSView, &name, &delete] {
            row.addArrangedSubview(view);
        }
        row
    }

    /// Opens the colour panel on the row's colour; what is chosen there becomes its own.
    fn choose_own(&self, row: isize) {
        let Some(space) = self.spaces().and_then(|list| usize::try_from(row).ok().and_then(|i| list.get(i).cloned())) else { return };
        // The menu shows the space's colour again until the panel gives one.
        *self.ivars().shown.borrow_mut() = None;
        self.refresh();
        if probe::enabled() {
            return;
        }
        let panel = NSColorPanel::sharedColorPanel(self.mtm());
        self.ivars().picking.set(Some(row));
        panel.setContinuous(false);
        panel.setShowsAlpha(false);
        panel.setColor(&crate::space_ui::color(space.color));
        // SAFETY: the tab lives as long as the settings window, kept with it.
        unsafe {
            panel.setTarget(Some(self));
            panel.setAction(Some(sel!(colorPicked:)));
        }
        panel.makeKeyAndOrderFront(None);
    }

    /// The vault's spaces as its root's rules file lists them now; none without a vault.
    fn spaces(&self) -> Option<Vec<Space>> {
        let vault = self.ivars().vault.borrow().clone()?;
        let rules = FsVault::new(&vault).read_rules(&[""]).unwrap_or_default();
        Some(Spaces::from_texts(&rules).list().to_vec())
    }

    /// The spaces with `change` made to the one in row `row`, written.
    fn edit(&self, row: isize, change: impl FnOnce(&mut Vec<Space>, usize)) {
        let Some(mut list) = self.spaces() else { return };
        let Some(i) = usize::try_from(row).ok().filter(|i| *i < list.len()) else { return };
        change(&mut list, i);
        self.write(&list);
    }

    /// Writes `list` into the root's rules file, its other keys kept, and shows it.
    fn write(&self, list: &[Space]) {
        let Some(vault) = self.ivars().vault.borrow().clone() else { return };
        let files = FsVault::new(&vault);
        // A rules file that cannot be read is not written over as if it were empty.
        let written = files
            .read_rules(&[""])
            .map(|r| r.into_iter().next().map(|(_, text)| text).unwrap_or_default())
            .map_err(|e| e.to_string())
            .and_then(|text| set_spaces(&text, list))
            .and_then(|text| files.write_rules("", &text).map_err(|e| e.to_string()));
        if let Err(e) = written {
            crate::rename_dialog::tell_failure(self.mtm(), None, strings::SPACES_FAILED, &e);
        }
        *self.ivars().shown.borrow_mut() = None;
        self.refresh();
    }

    /// Whether one of the rows' names has the keyboard (the rows wait to be rebuilt).
    fn editing(&self) -> bool {
        let Some(rows) = self.ivars().rows.borrow().clone() else { return false };
        let Some(responder) = rows.window().and_then(|w| w.firstResponder()) else { return false };
        // A field being edited hands the keyboard to its field editor, a text view inside it.
        responder.downcast::<objc2_app_kit::NSTextView>().ok().and_then(|editor| editor.delegate()).is_some_and(|delegate| {
            let field: &AnyObject = delegate.as_ref();
            rows.arrangedSubviews().iter().any(|row| row.subviews().iter().any(|v| std::ptr::eq(v.as_ref() as *const AnyObject, field as *const AnyObject)))
        })
    }

    /// The tab for the probe: the vault and each space with its colour.
    pub fn probe_line(&self) -> String {
        let vault = self.ivars().vault.borrow().clone().map(|v| vault_name(&v)).unwrap_or_default();
        let spaces: Vec<String> = self.ivars().shown.borrow().iter().flatten().map(|s| format!("{} ({})", s.name, s.color.name())).collect();
        std::iter::once(vault).chain(spaces).collect::<Vec<_>>().join(" | ")
    }

    /// Each row's colour as its menu shows it, for the probe: "Синій | Свій колір".
    pub fn probe_colors(&self) -> String {
        let Some(rows) = self.ivars().rows.borrow().clone() else { return String::new() };
        let titles: Vec<String> = rows
            .arrangedSubviews()
            .iter()
            .filter_map(|row| row.downcast::<NSStackView>().ok()?.arrangedSubviews().iter().next()?.downcast::<NSPopUpButton>().ok()?.titleOfSelectedItem())
            .map(|t| t.to_string())
            .collect();
        titles.join(" | ")
    }

    /// An edit through the tab's controls, as the user makes it (probe mode): `add`,
    /// `rename:0=Name`, `color:1=red` or `delete:0`.
    pub fn probe_edit(&self, spec: &str) {
        let Some(rows) = self.ivars().rows.borrow().clone() else { return };
        let control = |row: usize, at: usize| rows.arrangedSubviews().iter().nth(row).and_then(|r| r.downcast::<NSStackView>().ok()).and_then(|r| r.arrangedSubviews().iter().nth(at));
        let target: &AnyObject = self.as_ref();
        let (what, rest) = spec.split_once(':').unwrap_or((spec, ""));
        let (row, value) = rest.split_once('=').map_or((rest, ""), |(r, v)| (r, v));
        let row: usize = row.parse().unwrap_or(0);
        // SAFETY: each control's own action, sent to the tab as the control does.
        unsafe {
            match what {
                "add" => {
                    let _: () = msg_send![target, addSpace: Option::<&AnyObject>::None];
                }
                "rename" => {
                    if let Some(field) = control(row, 1).and_then(|v| v.downcast::<NSTextField>().ok()) {
                        field.setStringValue(&NSString::from_str(value));
                        let _: () = msg_send![target, renameSpace: &*field];
                    }
                }
                "color" => {
                    if let Some(popup) = control(row, 0).and_then(|v| v.downcast::<NSPopUpButton>().ok()) {
                        if value.starts_with('#') {
                            // One's own: «Свій колір…», then the panel's colour (not shown here).
                            popup.selectItemWithTag(CHOOSE_TAG);
                            let _: () = msg_send![target, colorSpace: &*popup];
                            self.ivars().picking.set(Some(row as isize));
                            let color = SpaceColor::from_name(value);
                            let panel = NSColorPanel::sharedColorPanel(self.mtm());
                            panel.setColor(&crate::space_ui::color(color));
                            let _: () = msg_send![target, colorPicked: &*panel];
                        } else {
                            popup.selectItemWithTag(SpaceColor::ALL.iter().position(|c| c.name() == value).unwrap_or(0) as isize);
                            let _: () = msg_send![target, colorSpace: &*popup];
                        }
                    }
                }
                "delete" => {
                    if let Some(button) = control(row, 2).and_then(|v| v.downcast::<NSButton>().ok()) {
                        let _: () = msg_send![target, deleteSpace: &*button];
                    }
                }
                _ => {}
            }
        }
    }
}

/// A vault's name: its folder's.
fn vault_name(vault: &Path) -> String {
    vault.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
