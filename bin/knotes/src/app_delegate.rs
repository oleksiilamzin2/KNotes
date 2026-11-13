use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use dispatch2::{DispatchQueue, MainThreadBound};
use knotes_session::{AppSettings, Event, VaultSession};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSModalResponseOK, NSOpenPanel, NSWindow, NSWindowDelegate};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol, NSString};

use crate::vault_window::VaultWindow;
use crate::{main_menu, probe, strings};

/// An open vault: its window and the session indexing it.
struct OpenVault {
    path: PathBuf,
    window: Rc<VaultWindow>,
    _session: VaultSession,
}

#[derive(Default)]
pub struct Ivars {
    vaults: RefCell<Vec<OpenVault>>,
    /// Window shown when no vault could be opened (probe mode and first launch).
    empty: RefCell<Option<Rc<VaultWindow>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesAppDelegate"]
    #[ivars = Ivars]
    pub struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            main_menu::install(&app, mtm);
            let start = std::env::args_os()
                .nth(1)
                .or_else(|| std::env::var_os("KNOTES_VAULT"))
                .map(PathBuf::from)
                .or_else(|| AppSettings::load(&app_data()).last_vault.filter(|p| p.is_dir()));
            match start {
                Some(vault) => self.open_vault(&vault),
                None if probe::enabled() => {
                    let window = VaultWindow::new(mtm);
                    window.window.makeKeyAndOrderFront(None);
                    probe::watch(&window);
                    probe::schedule(false);
                    *self.ivars().empty.borrow_mut() = Some(window);
                }
                None => self.ask_for_vault(),
            }
            if !probe::enabled() {
                app.activate();
            }
        }

        /// Dock click or `open` while running without windows: reopen the last vault.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _app: &NSApplication, has_visible_windows: bool) -> bool {
            if !has_visible_windows {
                match AppSettings::load(&app_data()).last_vault.filter(|p| p.is_dir()) {
                    Some(vault) => self.open_vault(&vault),
                    None => self.ask_for_vault(),
                }
            }
            true
        }

        /// Quitting writes every unsaved note first.
        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _notification: &NSNotification) {
            for vault in self.ivars().vaults.borrow().iter() {
                vault.window.leave();
            }
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        /// Closing a vault window stops its session and releases the vault lock.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            let Some(closing) = notification.object() else { return };
            let is_closing = |v: &OpenVault| std::ptr::eq(&*v.window.window as *const NSWindow as *const AnyObject, &*closing as *const AnyObject);
            if let Some(vault) = self.ivars().vaults.borrow().iter().find(|v| is_closing(v)) {
                vault.window.leave();
            }
            self.ivars().vaults.borrow_mut().retain(|v| !std::ptr::eq(&*v.window.window as *const NSWindow as *const AnyObject, &*closing as *const AnyObject));
        }
    }

    impl AppDelegate {
        /// File → Open Vault…
        #[unsafe(method(openVault:))]
        fn open_vault_action(&self, _sender: Option<&AnyObject>) {
            self.ask_for_vault();
        }

        /// File → New Note: in the window in front.
        #[unsafe(method(newNote:))]
        fn new_note_action(&self, _sender: Option<&AnyObject>) {
            if let Some(window) = self.front_window() {
                window.new_note();
            }
        }

        /// Edit → Find…: the find bar over the open note, wherever the focus is (the text view
        /// takes it itself when focused).
        #[unsafe(method(performTextFinderAction:))]
        fn find_in_note(&self, sender: Option<&AnyObject>) {
            if let Some(window) = self.front_window() {
                window.find_in_note(sender);
            }
        }

        /// Edit → Search Notes: the search over the note list.
        #[unsafe(method(searchNotes:))]
        fn search_notes(&self, _sender: Option<&AnyObject>) {
            if let Some(window) = self.front_window() {
                window.show_search();
            }
        }

        /// File → Save: writes the unsaved notes of all windows (autosave would in 2 s anyway).
        #[unsafe(method(saveDocument:))]
        fn save_document(&self, _sender: Option<&AnyObject>) {
            self.save_all();
        }
    }
);

impl AppDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        unsafe { msg_send![super(this), init] }
    }

    /// The key (or main) vault window; the first one when the app is not active.
    fn front_window(&self) -> Option<Rc<VaultWindow>> {
        let app = NSApplication::sharedApplication(self.mtm());
        let front = app.keyWindow().or_else(|| app.mainWindow());
        let vaults = self.ivars().vaults.borrow();
        let is_front = |v: &&OpenVault| front.as_ref().is_some_and(|f| std::ptr::eq(&*v.window.window, &**f));
        vaults.iter().find(is_front).or_else(|| vaults.first()).map(|v| Rc::clone(&v.window))
    }

    fn save_all(&self) {
        for vault in self.ivars().vaults.borrow().iter() {
            vault.window.save();
        }
    }

    fn ask_for_vault(&self) {
        let panel = NSOpenPanel::openPanel(self.mtm());
        panel.setCanChooseDirectories(true);
        panel.setCanChooseFiles(false);
        panel.setAllowsMultipleSelection(false);
        panel.setPrompt(Some(&NSString::from_str(strings::OPEN)));
        if panel.runModal() == NSModalResponseOK
            && let Some(path) = panel.URL().and_then(|url| url.path())
        {
            self.open_vault(Path::new(&path.to_string()));
        }
    }

    /// Opens a vault in its own window, or brings its window forward if it is open already.
    fn open_vault(&self, path: &Path) {
        let mtm = self.mtm();
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(open) = self.ivars().vaults.borrow().iter().find(|v| v.path == path) {
            open.window.window.makeKeyAndOrderFront(None);
            return;
        }
        let session = match VaultSession::open(&path, &app_data()) {
            Ok(session) => session,
            Err(e) => {
                eprintln!("knotes: cannot open {}: {e}", path.display());
                return;
            }
        };
        let (session, events) = session;
        let window = VaultWindow::new(mtm);
        window.set_vault(path.clone(), session.index_dir().to_path_buf());
        window.window.setDelegate(Some(ProtocolObject::from_ref(self)));
        window.window.makeKeyAndOrderFront(None);
        if probe::enabled() {
            probe::watch(&window);
        }
        let mut settings = AppSettings::load(&app_data());
        window.set_sidebar_mode(settings.sidebar_mode);
        window.on_sidebar_mode(|mode| {
            let mut settings = AppSettings::load(&app_data());
            settings.sidebar_mode = mode;
            if let Err(e) = settings.save(&app_data()) {
                eprintln!("knotes: cannot save settings: {e}");
            }
        });
        forward(events, &window, mtm);
        settings.last_vault = Some(path.clone());
        if let Err(e) = settings.save(&app_data()) {
            eprintln!("knotes: cannot save settings: {e}");
        }
        self.ivars().vaults.borrow_mut().push(OpenVault { path, window, _session: session });
    }
}

fn app_data() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/Knotes")
}

/// Moves session events to the main thread with GCD (no polling timer, so an idle app sleeps).
fn forward(events: Receiver<Event>, window: &Rc<VaultWindow>, mtm: MainThreadMarker) {
    let target = Arc::new(MainThreadBound::new(Rc::clone(window), mtm));
    std::thread::Builder::new()
        .name("knotes-events".into())
        .spawn(move || {
            for event in events {
                let target = Arc::clone(&target);
                DispatchQueue::main().exec_async(move || {
                    let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                    target.get(mtm).apply(event);
                });
            }
        })
        .expect("event thread starts");
}
