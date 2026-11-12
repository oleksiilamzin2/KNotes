use std::cell::{OnceCell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use dispatch2::{DispatchQueue, MainThreadBound};
use knotes_session::{Event, VaultSession};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol};

use crate::probe;
use crate::vault_window::VaultWindow;

#[derive(Default)]
pub struct Ivars {
    window: OnceCell<Rc<VaultWindow>>,
    session: RefCell<Option<VaultSession>>,
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
            let window = VaultWindow::new(mtm);
            if probe::enabled() {
                probe::watch(&window);
            }
            window.window.makeKeyAndOrderFront(None);
            let app = NSApplication::sharedApplication(mtm);
            // Probe runs from tests and must not take focus from the user.
            if !probe::enabled() {
                app.activate();
            }
            match vault_path() {
                Some(vault) => match VaultSession::open(&vault, &app_data()) {
                    Ok((session, events)) => {
                        window.set_vault(vault.clone(), session.index_dir().to_path_buf());
                        forward(events, &window, mtm);
                        *self.ivars().session.borrow_mut() = Some(session);
                    }
                    Err(e) => eprintln!("knotes: cannot open {}: {e}", vault.display()),
                },
                None if probe::enabled() => probe::schedule(false),
                None => {}
            }
            let _ = self.ivars().window.set(window);
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }
);

impl AppDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        unsafe { msg_send![super(this), init] }
    }
}

/// Vault from the first argument or `KNOTES_VAULT`; remembering the last vault comes later.
fn vault_path() -> Option<PathBuf> {
    std::env::args_os().nth(1).or_else(|| std::env::var_os("KNOTES_VAULT")).map(PathBuf::from)
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
