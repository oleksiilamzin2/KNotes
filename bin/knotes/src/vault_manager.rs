//! The vaults' window: on its left the vaults this Mac has opened (a click opens one, a
//! secondary click shows it in Finder or takes it off the list), on its right a new vault made
//! and a folder opened as a vault. The app's delegate keeps the list and opens the vaults.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSBox, NSBoxType, NSButton, NSColor, NSEvent, NSFont, NSFontWeightSemibold, NSMenu, NSMenuItem, NSModalResponseOK, NSOpenPanel, NSResponder, NSSavePanel, NSScrollView,
    NSStringDrawing, NSTextAlignment, NSTextField, NSView, NSWindow, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSSize, NSString, NSURL};

use crate::search_window::{attributes, fitted, rect};
use crate::strings;

const WIDTH: f64 = 760.0;
const HEIGHT: f64 = 420.0;
const LIST: f64 = 280.0;
const ROW: f64 = 52.0;

/// A vault known: its folder, and whether a window shows it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub path: PathBuf,
    pub open: bool,
}

impl Known {
    fn name(&self) -> String {
        self.path.file_name().map_or_else(|| self.path.display().to_string(), |name| name.to_string_lossy().into_owned())
    }
}

pub struct RowsIvars {
    vaults: RefCell<Vec<Known>>,
    on_open: RefCell<Option<Box<dyn Fn(usize)>>>,
    on_menu: RefCell<Option<Box<dyn Fn(usize, &NSEvent)>>>,
}

define_class!(
    /// The vaults known, a row each: its name over its folder.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesVaultRows"]
    #[ivars = RowsIvars]
    pub struct VaultRows;

    impl VaultRows {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: objc2_foundation::NSRect) {
            let width = self.bounds().size.width;
            let (name_font, path_font) = (NSFont::systemFontOfSize_weight(14.0, unsafe { NSFontWeightSemibold }), NSFont::systemFontOfSize(11.0));
            let (ink, quiet) = (NSColor::labelColor(), NSColor::secondaryLabelColor());
            for (at, vault) in self.ivars().vaults.borrow().iter().enumerate() {
                let top = at as f64 * ROW;
                // The vault a window shows now: a dot before its name.
                let x = if vault.open {
                    NSColor::controlAccentColor().setFill();
                    NSBezierPath::bezierPathWithOvalInRect(rect(16.0, top + 15.0, 7.0, 7.0)).fill();
                    30.0
                } else {
                    16.0
                };
                // SAFETY: a font and a colour are those attributes' values.
                unsafe {
                    NSString::from_str(&fitted(&vault.name(), &name_font, width - x - 12.0)).drawAtPoint_withAttributes(NSPoint::new(x, top + 8.0), Some(&attributes(&name_font, &ink)));
                    NSString::from_str(&fitted(&vault.path.display().to_string(), &path_font, width - 28.0)).drawAtPoint_withAttributes(NSPoint::new(16.0, top + 29.0), Some(&attributes(&path_font, &quiet)));
                }
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let (Some(row), Some(callback)) = (self.row_at(event), self.ivars().on_open.borrow().as_ref()) {
                callback(row);
            }
        }

        /// A secondary click on a row (two fingers, the right button): its menu.
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            if let (Some(row), Some(callback)) = (self.row_at(event), self.ivars().on_menu.borrow().as_ref()) {
                callback(row, event);
            }
        }
    }
);

impl VaultRows {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let ivars = RowsIvars { vaults: RefCell::new(Vec::new()), on_open: RefCell::new(None), on_menu: RefCell::new(None) };
        // SAFETY: a plain view's init.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(ivars)), init] }
    }

    fn row_at(&self, event: &NSEvent) -> Option<usize> {
        let at = self.convertPoint_fromView(event.locationInWindow(), None);
        let row = (at.y / ROW) as usize;
        (at.y >= 0.0 && row < self.ivars().vaults.borrow().len()).then_some(row)
    }
}

/// What the window asks of the app.
pub struct Asks {
    /// A vault of the list opened.
    pub open: Box<dyn Fn(&Path)>,
    /// A folder made a vault (a new one, or one chosen) and opened.
    pub start: Box<dyn Fn(&Path)>,
    /// A vault taken off the list.
    pub forget: Box<dyn Fn(&Path)>,
}

pub struct Ivars {
    window: Retained<NSWindow>,
    rows: Retained<VaultRows>,
    asks: RefCell<Option<Asks>>,
    /// The row a menu shows for.
    menu_row: RefCell<Option<PathBuf>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesVaultManager"]
    #[ivars = Ivars]
    pub struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    impl Controller {
        /// A new vault: a folder named and placed in a panel, made and opened.
        #[unsafe(method(createVault:))]
        fn create_vault(&self, _sender: Option<&AnyObject>) {
            let panel = NSSavePanel::savePanel(self.mtm());
            panel.setTitle(Some(&NSString::from_str(strings::VAULT_CREATE)));
            panel.setNameFieldStringValue(&NSString::from_str(strings::VAULT_NEW_NAME));
            panel.setPrompt(Some(&NSString::from_str(strings::VAULT_CREATE_BUTTON)));
            panel.setCanCreateDirectories(true);
            if panel.runModal() == NSModalResponseOK
                && let Some(path) = panel.URL().and_then(|url| url.path())
            {
                self.ask(|asks| (asks.start)(Path::new(&path.to_string())));
            }
        }

        /// A folder opened as a vault: chosen in a panel (or made there).
        #[unsafe(method(openFolder:))]
        fn open_folder(&self, _sender: Option<&AnyObject>) {
            let panel = NSOpenPanel::openPanel(self.mtm());
            panel.setCanChooseDirectories(true);
            panel.setCanChooseFiles(false);
            panel.setAllowsMultipleSelection(false);
            panel.setCanCreateDirectories(true);
            panel.setPrompt(Some(&NSString::from_str(strings::OPEN)));
            if panel.runModal() == NSModalResponseOK
                && let Some(path) = panel.URL().and_then(|url| url.path())
            {
                self.ask(|asks| (asks.start)(Path::new(&path.to_string())));
            }
        }

        #[unsafe(method(revealVault:))]
        fn reveal_vault(&self, _sender: Option<&AnyObject>) {
            if let Some(path) = self.ivars().menu_row.borrow().as_ref() {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
            }
        }

        #[unsafe(method(forgetVault:))]
        fn forget_vault(&self, _sender: Option<&AnyObject>) {
            let path = self.ivars().menu_row.borrow().clone();
            if let Some(path) = path {
                self.ask(|asks| (asks.forget)(&path));
            }
        }
    }
);

impl Controller {
    fn ask(&self, what: impl FnOnce(&Asks)) {
        if let Some(asks) = self.ivars().asks.borrow().as_ref() {
            what(asks);
        }
    }

    fn open_row(&self, row: usize) {
        let path = self.ivars().rows.ivars().vaults.borrow().get(row).map(|vault| vault.path.clone());
        if let Some(path) = path {
            self.ask(|asks| (asks.open)(&path));
        }
    }

    fn show_menu(&self, row: usize, event: &NSEvent) {
        let Some(path) = self.ivars().rows.ivars().vaults.borrow().get(row).map(|vault| vault.path.clone()) else { return };
        *self.ivars().menu_row.borrow_mut() = Some(path);
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        for (title, action) in [(strings::VAULT_REVEAL, sel!(revealVault:)), (strings::VAULT_FORGET, sel!(forgetVault:))] {
            // SAFETY: the controller outlives its menu and takes the action.
            let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(action), &NSString::new()) };
            // SAFETY: as above.
            unsafe { item.setTarget(Some(self)) };
            menu.addItem(&item);
        }
        NSMenu::popUpContextMenu_withEvent_forView(&menu, event, &self.ivars().rows);
    }
}

/// The vaults' window.
pub struct VaultManager {
    controller: Retained<Controller>,
}

impl VaultManager {
    pub fn new(mtm: MainThreadMarker, asks: Asks) -> Self {
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
        // SAFETY: a window made and kept by Rust (`setReleasedWhenClosed(false)`).
        let window = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT), style, NSBackingStoreType::Buffered, false) };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str(strings::VAULTS));
        crate::probe::conceal(&window);
        let rows = VaultRows::new(mtm);
        let ivars = Ivars { window: window.clone(), rows: rows.clone(), asks: RefCell::new(Some(asks)), menu_row: RefCell::new(None) };
        let controller: Retained<Controller> = unsafe { msg_send![super(Controller::alloc(mtm).set_ivars(ivars)), init] };
        let content = window.contentView().expect("a window has a content view");
        // The list on the left, a line beside it. Frames are from the bottom up.
        let scroll = NSScrollView::new(mtm);
        scroll.setFrame(rect(0.0, 0.0, LIST, HEIGHT));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setDocumentView(Some(&rows));
        content.addSubview(&scroll);
        let line = NSBox::new(mtm);
        line.setBoxType(NSBoxType::Separator);
        line.setFrame(rect(LIST, 0.0, 1.0, HEIGHT));
        content.addSubview(&line);
        // On the right: the app's name, then what can be done, a row each.
        let (left, wide) = (LIST + 40.0, WIDTH - LIST - 80.0);
        let label = |text: &str, font: &NSFont, colour: &NSColor, frame: objc2_foundation::NSRect, centred: bool| {
            let field = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
            field.setFont(Some(font));
            field.setTextColor(Some(colour));
            field.setFrame(frame);
            if centred {
                field.setAlignment(NSTextAlignment::Center);
            }
            content.addSubview(&field);
        };
        label("Knotes", &NSFont::systemFontOfSize_weight(30.0, unsafe { NSFontWeightSemibold }), &NSColor::labelColor(), rect(left, HEIGHT - 96.0, wide, 40.0), true);
        label(&strings::version(env!("CARGO_PKG_VERSION")), &NSFont::systemFontOfSize(12.0), &NSColor::secondaryLabelColor(), rect(left, HEIGHT - 120.0, wide, 18.0), true);
        let acts = [
            (strings::VAULT_CREATE, strings::VAULT_CREATE_DETAILS, strings::VAULT_CREATE_BUTTON, sel!(createVault:)),
            (strings::VAULT_OPEN_FOLDER, strings::VAULT_OPEN_FOLDER_DETAILS, strings::OPEN, sel!(openFolder:)),
        ];
        for (at, (title, details, name, action)) in acts.into_iter().enumerate() {
            let top = 170.0 + at as f64 * 96.0;
            if at > 0 {
                let line = NSBox::new(mtm);
                line.setBoxType(NSBoxType::Separator);
                line.setFrame(rect(left, HEIGHT - top + 14.0, wide, 1.0));
                content.addSubview(&line);
            }
            label(title, &NSFont::systemFontOfSize(14.0), &NSColor::labelColor(), rect(left, HEIGHT - top - 20.0, wide - 130.0, 20.0), false);
            label(details, &NSFont::systemFontOfSize(12.0), &NSColor::secondaryLabelColor(), rect(left, HEIGHT - top - 58.0, wide - 130.0, 34.0), false);
            // SAFETY: the controller is kept as long as the window and takes the action.
            let button = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(name), Some(&controller), Some(action), mtm) };
            button.setFrame(rect(left + wide - 116.0, HEIGHT - top - 36.0, 116.0, 30.0));
            content.addSubview(&button);
        }
        let weak = objc2::rc::Weak::from_retained(&controller);
        *rows.ivars().on_open.borrow_mut() = Some(Box::new(move |row| {
            if let Some(controller) = weak.load() {
                controller.open_row(row);
            }
        }));
        let weak = objc2::rc::Weak::from_retained(&controller);
        *rows.ivars().on_menu.borrow_mut() = Some(Box::new(move |row, event| {
            if let Some(controller) = weak.load() {
                controller.show_menu(row, event);
            }
        }));
        Self { controller }
    }

    /// The window shown with the vaults `vaults`.
    pub fn show(&self, vaults: Vec<Known>) {
        self.set_vaults(vaults);
        let window = &self.controller.ivars().window;
        if !window.isVisible() && !crate::probe::enabled() {
            window.center();
        }
        window.makeKeyAndOrderFront(None);
    }

    /// The vaults listed are `vaults` now.
    pub fn set_vaults(&self, vaults: Vec<Known>) {
        let rows = &self.controller.ivars().rows;
        rows.setFrameSize(NSSize::new(LIST, (vaults.len() as f64 * ROW).max(HEIGHT)));
        *rows.ivars().vaults.borrow_mut() = vaults;
        rows.setNeedsDisplay(true);
    }

    pub fn hide(&self) {
        self.controller.ivars().window.orderOut(None);
    }

    /// The list for the probe: "Нотатки (open) — /path | Робота — /path".
    pub fn describe(&self) -> String {
        let vaults = self.controller.ivars().rows.ivars().vaults.borrow();
        vaults.iter().map(|vault| format!("{}{} — {}", vault.name(), if vault.open { " (open)" } else { "" }, vault.path.display())).collect::<Vec<_>>().join(" | ")
    }

    /// A click on the row at `row` (probe mode).
    pub fn probe_open(&self, row: usize) {
        self.controller.open_row(row);
    }

    /// The row's menu → off the list (probe mode).
    pub fn probe_forget(&self, row: usize) {
        let path = self.controller.ivars().rows.ivars().vaults.borrow().get(row).map(|vault| vault.path.clone());
        if let Some(path) = path {
            self.controller.ask(|asks| (asks.forget)(&path));
        }
    }
}
