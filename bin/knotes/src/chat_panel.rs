//! The chat in the right panel's "ШІ" segment (ADR 0027): signing in with ChatGPT, the
//! conversation and the list of them, the question field, the document offered to the model,
//! the plan's usage.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};
use knotes_chat::{
    ChatError, ChatEvent, Conversation, ConversationEntry, Conversations, CredentialStore, Credentials, KeychainStore, MemoryStore, Model, Registration, Run, RunLink, Shown, SignIn, SignInError, Transport, UreqTransport,
    list_models, reply_runs,
};
use knotes_session::AppSettings;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{AllocAnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSBezelStyle, NSBox, NSBoxType, NSButton, NSColor, NSControlSize, NSEvent, NSEventModifierFlags, NSFont, NSFontWeightRegular, NSFontWeightSemibold, NSImage, NSLayoutConstraint, NSLineBreakMode, NSMenu, NSMenuItem,
    NSMutableParagraphStyle, NSPopUpButton, NSStackView, NSTextDelegate, NSTextField, NSTextView, NSTextViewDelegate, NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView, NSWorkspace,
};
use objc2_foundation::{NSArray, NSAttributedString, NSDictionary, NSMutableAttributedString, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString, NSUInteger, NSURL, ns_string};

use crate::chat_worker::{Account, ChatWorker, ScriptTransport, Turn};
use crate::strings;

/// Where a link in the conversation goes, as its attribute keeps it.
const NOTE_LINK: &str = "knotes-note:";
const WEB_LINK: &str = "knotes-web:";

type OnOpenLink = Box<dyn Fn(String)>;

pub struct TargetIvars {
    panel: RefCell<Weak<ChatPanel>>,
}

define_class!(
    /// The panel's buttons' target and its text views' delegate.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesChatTarget"]
    #[ivars = TargetIvars]
    pub struct ChatTarget;

    unsafe impl NSObjectProtocol for ChatTarget {}

    unsafe impl NSTextDelegate for ChatTarget {
        #[unsafe(method(textDidChange:))]
        fn text_did_change(&self, _notification: &NSNotification) {
            if let Some(panel) = self.panel() {
                panel.composer_changed();
            }
        }
    }

    unsafe impl NSTextViewDelegate for ChatTarget {
        #[unsafe(method(textView:doCommandBySelector:))]
        fn do_command(&self, view: &NSTextView, selector: Sel) -> bool {
            let shift = NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Shift);
            let send = selector == sel!(insertNewline:) && !shift;
            let panel = self.panel().filter(|p| send && std::ptr::eq(view, &*p.composer));
            if let Some(panel) = &panel {
                panel.send();
            }
            panel.is_some()
        }

        #[unsafe(method(textView:clickedOnLink:atIndex:))]
        fn clicked_link(&self, _view: &NSTextView, link: &AnyObject, _index: NSUInteger) -> bool {
            let link = link.downcast_ref::<NSString>().map(|s| s.to_string()).unwrap_or_default();
            if let Some(panel) = self.panel() {
                panel.follow(&link);
            }
            true
        }
    }

    impl ChatTarget {
        #[unsafe(method(signIn:))]
        fn sign_in(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.sign_in();
            }
        }

        #[unsafe(method(cancelSignIn:))]
        fn cancel_sign_in(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.cancel_sign_in();
            }
        }

        #[unsafe(method(send:))]
        fn send_pressed(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.send();
            }
        }

        #[unsafe(method(newConversation:))]
        fn new_conversation(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.new_conversation();
            }
        }

        #[unsafe(method(chooseConversation:))]
        fn choose_conversation(&self, sender: &NSPopUpButton) {
            if let Some(panel) = self.panel() {
                panel.choose_conversation(sender.indexOfSelectedItem());
            }
        }

        #[unsafe(method(chooseModel:))]
        fn choose_model(&self, sender: &NSPopUpButton) {
            if let Some(panel) = self.panel() {
                panel.choose_model(sender.indexOfSelectedItem());
            }
        }

        #[unsafe(method(toggleDocument:))]
        fn toggle_document(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.toggle_document();
            }
        }

        #[unsafe(method(manageUsage:))]
        fn manage_usage(&self, _sender: Option<&AnyObject>) {
            open_url(strings::CHAT_USAGE_URL);
        }

        #[unsafe(method(showMore:))]
        fn show_more(&self, sender: &NSButton) {
            if let Some(panel) = self.panel() {
                panel.show_more(sender);
            }
        }

        #[unsafe(method(deleteConversation:))]
        fn delete_conversation(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.delete_conversation();
            }
        }

        #[unsafe(method(signOut:))]
        fn sign_out(&self, _sender: Option<&AnyObject>) {
            if let Some(panel) = self.panel() {
                panel.sign_out();
            }
        }
    }
);

impl ChatTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { panel: RefCell::new(Weak::new()) });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }

    fn panel(&self) -> Option<Rc<ChatPanel>> {
        self.ivars().panel.borrow().upgrade()
    }
}

/// A turn under way: the question, the reply so far and the tools it called.
struct Asking {
    question: String,
    reply: String,
    tools: Vec<(String, serde_json::Value)>,
    cancel: Arc<AtomicBool>,
}

struct State {
    registration: Registration,
    conversations: Option<Conversations>,
    /// The conversations as the list shows them, newest first.
    entries: Vec<ConversationEntry>,
    current: Conversation,
    document: Option<String>,
    /// The document is offered to the model (the chip under the field not crossed out).
    document_on: bool,
    models: Vec<Model>,
    model: String,
    turn: Option<Asking>,
    /// What went wrong in the last turn.
    error: Option<ChatError>,
    /// Set to call off the sign-in under way.
    sign_in: Option<Arc<AtomicBool>>,
    sign_in_words: String,
}

pub struct ChatPanel {
    pub view: Retained<NSView>,
    signed_out: Retained<NSView>,
    signed_in: Retained<NSView>,
    sign_in_button: Retained<NSButton>,
    sign_in_status: Retained<NSTextField>,
    cancel_sign_in: Retained<NSButton>,
    conversations_popup: Retained<NSPopUpButton>,
    transcript: Retained<NSTextView>,
    composer: Retained<NSTextView>,
    placeholder: Retained<NSTextField>,
    send_button: Retained<NSButton>,
    document_chip: Retained<NSButton>,
    model_popup: Retained<NSPopUpButton>,
    target: Retained<ChatTarget>,
    state: RefCell<State>,
    render_due: Cell<bool>,
    account: Arc<Account>,
    worker: ChatWorker,
    app_data: PathBuf,
    this: RefCell<Weak<ChatPanel>>,
    on_open_link: RefCell<Option<OnOpenLink>>,
}

impl ChatPanel {
    pub fn new(mtm: MainThreadMarker, app_data: PathBuf) -> Rc<Self> {
        let account = Arc::new(account());
        let target = ChatTarget::new(mtm);
        let view = NSView::new(mtm);

        // Signed out: what the chat is, and the button.
        let signed_out = NSView::new(mtm);
        let title = NSTextField::labelWithString(&NSString::from_str(strings::CHAT), mtm);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(NSFont::systemFontSize() + 2.0)));
        let explained = NSTextField::wrappingLabelWithString(&NSString::from_str(strings::CHAT_EXPLAINED), mtm);
        explained.setTextColor(Some(&NSColor::secondaryLabelColor()));
        explained.setPreferredMaxLayoutWidth(220.0);
        // SAFETY: the target outlives the button (the panel keeps both).
        let sign_in_button = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::CHAT_SIGN_IN), Some(&target), Some(sel!(signIn:)), mtm) };
        sign_in_button.setBezelColor(Some(&NSColor::blackColor()));
        sign_in_button.setControlSize(NSControlSize::Large);
        let sign_in_status = NSTextField::wrappingLabelWithString(ns_string!(""), mtm);
        sign_in_status.setTextColor(Some(&NSColor::secondaryLabelColor()));
        sign_in_status.setPreferredMaxLayoutWidth(220.0);
        // SAFETY: as above.
        let cancel_sign_in = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::CHAT_CANCEL), Some(&target), Some(sel!(cancelSignIn:)), mtm) };
        cancel_sign_in.setHidden(true);
        let views: [&NSView; 5] = [&title, &explained, &sign_in_button, &sign_in_status, &cancel_sign_in];
        let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(&views), mtm);
        stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
        stack.setSpacing(12.0);
        add(&signed_out, &stack);
        activate(&[
            stack.centerYAnchor().constraintEqualToAnchor_constant(&signed_out.centerYAnchor(), -40.0),
            stack.leadingAnchor().constraintEqualToAnchor_constant(&signed_out.leadingAnchor(), 20.0),
            stack.trailingAnchor().constraintEqualToAnchor_constant(&signed_out.trailingAnchor(), -20.0),
        ]);

        // Signed in: the list of conversations, the conversation, the field and what is under it.
        let signed_in = NSView::new(mtm);
        let conversations_popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        conversations_popup.setBordered(false);
        conversations_popup.setFont(Some(&NSFont::systemFontOfSize_weight(NSFont::systemFontSize(), unsafe { NSFontWeightSemibold })));
        // SAFETY: the target outlives the controls (the panel keeps both).
        unsafe {
            conversations_popup.setTarget(Some(&target));
            conversations_popup.setAction(Some(sel!(chooseConversation:)));
        }
        let new_button = symbol_button("square.and.pencil", strings::CHAT_NEW, &target, sel!(newConversation:), mtm);
        let more_button = symbol_button("ellipsis.circle", "", &target, sel!(showMore:), mtm);
        let transcript_scroll = NSTextView::scrollableTextView(mtm);
        transcript_scroll.setDrawsBackground(false);
        let transcript = transcript_scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).expect("a scrollable text view holds a text view");
        transcript.setEditable(false);
        transcript.setSelectable(true);
        transcript.setDrawsBackground(false);
        transcript.setTextContainerInset(NSSize::new(10.0, 8.0));
        transcript.setDelegate(Some(ProtocolObject::from_ref(&*target)));

        let field_box = NSBox::new(mtm);
        field_box.setBoxType(NSBoxType::Custom);
        field_box.setTitlePosition(NSTitlePosition::NoTitle);
        field_box.setCornerRadius(10.0);
        field_box.setBorderWidth(1.0);
        field_box.setBorderColor(&NSColor::separatorColor());
        field_box.setFillColor(&NSColor::textBackgroundColor());
        field_box.setContentViewMargins(NSSize::ZERO);
        let composer_scroll = NSTextView::scrollableTextView(mtm);
        composer_scroll.setDrawsBackground(false);
        let composer = composer_scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).expect("a scrollable text view holds a text view");
        composer.setRichText(false);
        composer.setDrawsBackground(false);
        composer.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
        composer.setTextContainerInset(NSSize::new(4.0, 6.0));
        composer.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        let placeholder = NSTextField::labelWithString(&NSString::from_str(strings::ASK_NOTES), mtm);
        placeholder.setTextColor(Some(&NSColor::placeholderTextColor()));
        let send_button = symbol_button("arrow.up.circle.fill", "", &target, sel!(send:), mtm);
        let field = field_box.contentView().expect("a box has a content view");
        for v in [&*composer_scroll as &NSView, &placeholder, &send_button] {
            add(&field, v);
        }
        activate(&[
            composer_scroll.topAnchor().constraintEqualToAnchor_constant(&field.topAnchor(), 2.0),
            composer_scroll.bottomAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), -2.0),
            composer_scroll.leadingAnchor().constraintEqualToAnchor_constant(&field.leadingAnchor(), 4.0),
            composer_scroll.trailingAnchor().constraintEqualToAnchor_constant(&send_button.leadingAnchor(), -2.0),
            composer_scroll.heightAnchor().constraintEqualToConstant(66.0),
            placeholder.topAnchor().constraintEqualToAnchor_constant(&field.topAnchor(), 8.0),
            placeholder.leadingAnchor().constraintEqualToAnchor_constant(&field.leadingAnchor(), 13.0),
            send_button.trailingAnchor().constraintEqualToAnchor_constant(&field.trailingAnchor(), -6.0),
            send_button.bottomAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), -6.0),
        ]);

        // SAFETY: the target outlives the controls.
        let document_chip = unsafe { NSButton::buttonWithTitle_target_action(ns_string!(""), Some(&target), Some(sel!(toggleDocument:)), mtm) };
        document_chip.setBezelStyle(NSBezelStyle::Badge);
        document_chip.setControlSize(NSControlSize::Small);
        document_chip.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        document_chip.setHidden(true);
        let model_popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        model_popup.setBordered(false);
        model_popup.setControlSize(NSControlSize::Small);
        model_popup.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        // SAFETY: the target outlives the control.
        unsafe {
            model_popup.setTarget(Some(&target));
            model_popup.setAction(Some(sel!(chooseModel:)));
        }
        let plan = NSTextField::labelWithString(&NSString::from_str(strings::CHAT_PLAN), mtm);
        plan.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        plan.setTextColor(Some(&NSColor::secondaryLabelColor()));
        plan.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        // SAFETY: the target outlives the button.
        let manage = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(strings::CHAT_MANAGE), Some(&target), Some(sel!(manageUsage:)), mtm) };
        manage.setBordered(false);
        manage.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        manage.setContentTintColor(Some(&NSColor::linkColor()));

        for v in [&*conversations_popup as &NSView, &new_button, &more_button, &transcript_scroll, &field_box, &document_chip, &model_popup, &plan, &manage] {
            add(&signed_in, v);
        }
        let guide = signed_in.safeAreaLayoutGuide();
        activate(&[
            conversations_popup.topAnchor().constraintEqualToAnchor_constant(&guide.topAnchor(), 6.0),
            conversations_popup.leadingAnchor().constraintEqualToAnchor_constant(&signed_in.leadingAnchor(), 8.0),
            conversations_popup.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&new_button.leadingAnchor(), -4.0),
            more_button.centerYAnchor().constraintEqualToAnchor(&conversations_popup.centerYAnchor()),
            more_button.trailingAnchor().constraintEqualToAnchor_constant(&signed_in.trailingAnchor(), -10.0),
            new_button.centerYAnchor().constraintEqualToAnchor(&conversations_popup.centerYAnchor()),
            new_button.trailingAnchor().constraintEqualToAnchor_constant(&more_button.leadingAnchor(), -8.0),
            transcript_scroll.topAnchor().constraintEqualToAnchor_constant(&conversations_popup.bottomAnchor(), 4.0),
            transcript_scroll.leadingAnchor().constraintEqualToAnchor(&signed_in.leadingAnchor()),
            transcript_scroll.trailingAnchor().constraintEqualToAnchor(&signed_in.trailingAnchor()),
            transcript_scroll.bottomAnchor().constraintEqualToAnchor_constant(&field_box.topAnchor(), -6.0),
            field_box.leadingAnchor().constraintEqualToAnchor_constant(&signed_in.leadingAnchor(), 10.0),
            field_box.trailingAnchor().constraintEqualToAnchor_constant(&signed_in.trailingAnchor(), -10.0),
            field_box.bottomAnchor().constraintEqualToAnchor_constant(&model_popup.topAnchor(), -4.0),
            model_popup.trailingAnchor().constraintEqualToAnchor_constant(&signed_in.trailingAnchor(), -8.0),
            model_popup.bottomAnchor().constraintEqualToAnchor_constant(&plan.topAnchor(), -4.0),
            document_chip.leadingAnchor().constraintEqualToAnchor_constant(&signed_in.leadingAnchor(), 12.0),
            document_chip.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&model_popup.leadingAnchor(), -8.0),
            document_chip.centerYAnchor().constraintEqualToAnchor(&model_popup.centerYAnchor()),
            plan.leadingAnchor().constraintEqualToAnchor_constant(&signed_in.leadingAnchor(), 12.0),
            plan.bottomAnchor().constraintEqualToAnchor_constant(&signed_in.bottomAnchor(), -10.0),
            manage.leadingAnchor().constraintEqualToAnchor_constant(&plan.trailingAnchor(), 4.0),
            manage.trailingAnchor().constraintLessThanOrEqualToAnchor_constant(&signed_in.trailingAnchor(), -8.0),
            manage.centerYAnchor().constraintEqualToAnchor(&plan.centerYAnchor()),
        ]);
        plan.setContentCompressionResistancePriority_forOrientation(250.0, objc2_app_kit::NSLayoutConstraintOrientation::Horizontal);

        for container in [&signed_out, &signed_in] {
            add(&view, container);
            activate(&[
                container.topAnchor().constraintEqualToAnchor(&view.topAnchor()),
                container.bottomAnchor().constraintEqualToAnchor(&view.bottomAnchor()),
                container.leadingAnchor().constraintEqualToAnchor(&view.leadingAnchor()),
                container.trailingAnchor().constraintEqualToAnchor(&view.trailingAnchor()),
            ]);
        }

        let settings = AppSettings::load(&app_data);
        let model = settings.chat_model.unwrap_or_default();
        let state = State {
            registration: Registration::load(&app_data),
            conversations: None,
            entries: Vec::new(),
            current: Conversation::new(&model),
            document: None,
            document_on: true,
            models: Vec::new(),
            model,
            turn: None,
            error: None,
            sign_in: None,
            sign_in_words: String::new(),
        };
        let worker = ChatWorker::new(Arc::clone(&account));
        let panel = Rc::new(Self {
            view,
            signed_out,
            signed_in,
            sign_in_button,
            sign_in_status,
            cancel_sign_in,
            conversations_popup,
            transcript,
            composer,
            placeholder,
            send_button,
            document_chip,
            model_popup,
            target,
            state: RefCell::new(state),
            render_due: Cell::new(false),
            account,
            worker,
            app_data,
            this: RefCell::new(Weak::new()),
            on_open_link: RefCell::new(None),
        });
        *panel.this.borrow_mut() = Rc::downgrade(&panel);
        *panel.target.ivars().panel.borrow_mut() = Rc::downgrade(&panel);
        panel.show_account();
        panel
    }

    /// Called with a note link clicked in the conversation (its target as written).
    pub fn on_open_link(&self, callback: impl Fn(String) + 'static) {
        *self.on_open_link.borrow_mut() = Some(Box::new(callback));
    }

    /// The vault open: its conversations listed, the newest shown, its tools for the model.
    pub fn set_vault(&self, vault: PathBuf, index_dir: PathBuf) {
        self.worker.set_vault(vault, index_dir.clone());
        let conversations = Conversations::new(index_dir.join("conversations"));
        {
            let mut state = self.state.borrow_mut();
            state.entries = conversations.list();
            let model = state.model.clone();
            state.current = state.entries.first().and_then(|e| conversations.load(&e.id)).unwrap_or_else(|| Conversation::new(&model));
            state.conversations = Some(conversations);
            state.error = None;
        }
        self.list_conversations();
        self.render();
    }

    /// The document open (a vault path), offered to the model unless crossed out.
    pub fn set_document(&self, document: Option<String>) {
        {
            let mut state = self.state.borrow_mut();
            if state.document == document {
                return;
            }
            state.document = document;
            state.document_on = true;
        }
        self.show_document();
    }

    fn mtm(&self) -> MainThreadMarker {
        self.target.mtm()
    }

    /// The signed-in or the signed-out view, as the account is.
    fn show_account(&self) {
        let signed_in = self.account.is_signed_in();
        self.signed_in.setHidden(!signed_in);
        self.signed_out.setHidden(signed_in);
        let waiting = self.state.borrow().sign_in.is_some();
        self.sign_in_button.setEnabled(!waiting);
        self.cancel_sign_in.setHidden(!waiting);
        let words = if waiting { strings::CHAT_WAITING.to_string() } else { self.state.borrow().sign_in_words.clone() };
        self.sign_in_status.setStringValue(&NSString::from_str(&words));
        self.sign_in_status.setHidden(words.is_empty());
        if signed_in && self.state.borrow().models.is_empty() {
            self.list_models();
        }
        self.show_document();
        self.composer_changed();
    }

    // Signing in and out.

    fn sign_in(&self) {
        if self.state.borrow().sign_in.is_some() {
            return;
        }
        let registration = self.state.borrow().registration.clone();
        let attempt = match SignIn::start(&registration, self.account.id_token_hint().as_deref()) {
            Ok(attempt) => attempt,
            Err(e) => {
                self.state.borrow_mut().sign_in_words = e.to_string();
                return self.show_account();
            }
        };
        open_url(&attempt.url());
        let cancel = Arc::new(AtomicBool::new(false));
        self.state.borrow_mut().sign_in = Some(Arc::clone(&cancel));
        self.show_account();
        let account = Arc::clone(&self.account);
        let target = MainThreadBound::new(self.this.borrow().clone(), self.mtm());
        std::thread::spawn(move || {
            let result = attempt.complete(account.transport(), &cancel);
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                if let Some(panel) = target.get(mtm).upgrade() {
                    panel.signed_in_with(result);
                }
            });
        });
    }

    fn cancel_sign_in(&self) {
        if let Some(cancel) = self.state.borrow_mut().sign_in.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.show_account();
    }

    fn signed_in_with(&self, result: Result<Credentials, SignInError>) {
        self.state.borrow_mut().sign_in = None;
        let words = match result {
            Ok(credentials) => {
                let first = {
                    let mut state = self.state.borrow_mut();
                    let registration = &mut state.registration;
                    registration.client_id = Some(credentials.client_id.clone());
                    registration.email = credentials.email.clone();
                    registration.subject = Some(credentials.subject.clone());
                    let first = !registration.welcomed;
                    registration.welcomed = true;
                    if let Err(e) = registration.save(&self.app_data) {
                        eprintln!("knotes: cannot save the ChatGPT registration: {e}");
                    }
                    first
                };
                match self.account.accept(credentials) {
                    Ok(()) => {
                        if first {
                            self.welcome();
                        }
                        String::new()
                    }
                    Err(e) => e,
                }
            }
            Err(SignInError::Cancelled) => String::new(),
            Err(SignInError::Declined) => strings::CHAT_DECLINED.to_string(),
            Err(SignInError::PlanNotEnabled) => strings::CHAT_PLAN_OFF.to_string(),
            Err(e) => e.to_string(),
        };
        self.state.borrow_mut().sign_in_words = words;
        self.show_account();
    }

    /// The first sign-in's notice that the plan is used (OpenAI asks for it once).
    fn welcome(&self) {
        let alert = NSAlert::new(self.mtm());
        alert.setMessageText(&NSString::from_str(strings::CHAT_WELCOME));
        alert.setInformativeText(&NSString::from_str(strings::CHAT_WELCOME_EXPLAINED));
        alert.addButtonWithTitle(&NSString::from_str(strings::CHAT_GOT_IT));
        match self.view.window() {
            Some(window) => alert.beginSheetModalForWindow_completionHandler(&window, None),
            None => {
                alert.runModal();
            }
        }
    }

    fn sign_out(&self) {
        if let Some(turn) = &self.state.borrow().turn {
            turn.cancel.store(true, Ordering::Relaxed);
        }
        let account = Arc::clone(&self.account);
        let target = MainThreadBound::new(self.this.borrow().clone(), self.mtm());
        std::thread::spawn(move || {
            account.sign_out();
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                if let Some(panel) = target.get(mtm).upgrade() {
                    panel.state.borrow_mut().models.clear();
                    panel.show_account();
                }
            });
        });
    }

    // Models.

    fn list_models(&self) {
        let account = Arc::clone(&self.account);
        let target = MainThreadBound::new(self.this.borrow().clone(), self.mtm());
        std::thread::spawn(move || {
            let models = list_models(account.transport(), &*account);
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                if let (Some(panel), Ok(models)) = (target.get(mtm).upgrade(), models) {
                    panel.set_models(models);
                }
            });
        });
    }

    fn set_models(&self, models: Vec<Model>) {
        {
            let mut state = self.state.borrow_mut();
            if !models.iter().any(|m| m.slug == state.model) {
                state.model = models.first().map(|m| m.slug.clone()).unwrap_or_default();
            }
            state.models = models;
        }
        self.show_models();
    }

    fn show_models(&self) {
        let state = self.state.borrow();
        self.model_popup.removeAllItems();
        for model in &state.models {
            self.model_popup.addItemWithTitle(&NSString::from_str(&model.name));
        }
        if let Some(i) = state.models.iter().position(|m| m.slug == state.model) {
            self.model_popup.selectItemAtIndex(i as isize);
        }
    }

    fn choose_model(&self, index: isize) {
        let slug = {
            let mut state = self.state.borrow_mut();
            let Some(model) = usize::try_from(index).ok().and_then(|i| state.models.get(i)).map(|m| m.slug.clone()) else { return };
            state.model = model.clone();
            model
        };
        let mut settings = AppSettings::load(&self.app_data);
        settings.chat_model = Some(slug);
        if let Err(e) = settings.save(&self.app_data) {
            eprintln!("knotes: cannot save settings: {e}");
        }
    }

    // Conversations.

    /// The list shows the conversations, a new one not saved yet first.
    fn list_conversations(&self) {
        let state = self.state.borrow();
        self.conversations_popup.removeAllItems();
        let saved = state.entries.iter().position(|e| e.id == state.current.id);
        if saved.is_none() {
            self.conversations_popup.addItemWithTitle(&NSString::from_str(strings::CHAT_NEW));
        }
        for (i, entry) in state.entries.iter().enumerate() {
            let title = if entry.title.is_empty() { strings::CHAT_UNTITLED } else { &entry.title };
            // Equal titles would fold into one item.
            self.conversations_popup.addItemWithTitle(&NSString::from_str(&format!("{title}{}", "\u{200b}".repeat(i))));
            if let Some(item) = self.conversations_popup.lastItem() {
                item.setTitle(&NSString::from_str(title));
            }
        }
        self.conversations_popup.selectItemAtIndex(saved.map_or(0, |i| i as isize));
        self.conversations_popup.setEnabled(state.turn.is_none());
    }

    fn choose_conversation(&self, index: isize) {
        {
            let mut state = self.state.borrow_mut();
            let offset = usize::from(!state.entries.iter().any(|e| e.id == state.current.id));
            let Some(entry) = usize::try_from(index).ok().and_then(|i| i.checked_sub(offset)).and_then(|i| state.entries.get(i)).cloned() else { return };
            let Some(conversation) = state.conversations.as_ref().and_then(|c| c.load(&entry.id)) else { return };
            state.current = conversation;
            state.error = None;
        }
        self.list_conversations();
        self.render();
    }

    fn new_conversation(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.turn.is_some() || state.current.items.is_empty() {
                return;
            }
            state.current = Conversation::new(&state.model);
            state.error = None;
        }
        self.list_conversations();
        self.render();
        if let Some(window) = self.view.window() {
            window.makeFirstResponder(Some(&self.composer));
        }
    }

    fn delete_conversation(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.turn.is_some() {
                return;
            }
            let id = state.current.id.clone();
            let Some(conversations) = state.conversations.as_ref() else { return };
            conversations.delete(&id);
            let entries = conversations.list();
            let next = entries.first().and_then(|e| conversations.load(&e.id));
            state.entries = entries;
            let model = state.model.clone();
            state.current = next.unwrap_or_else(|| Conversation::new(&model));
            state.error = None;
        }
        self.list_conversations();
        self.render();
    }

    fn show_more(&self, button: &NSButton) {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        let item = |title: &str, action: Option<Sel>| {
            // SAFETY: a menu item with the panel's target, which outlives the menu.
            let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), action, ns_string!("")) };
            // SAFETY: as above.
            unsafe { item.setTarget(Some(&self.target)) };
            item.setEnabled(action.is_some());
            menu.addItem(&item);
        };
        item(strings::CHAT_DELETE, (!self.state.borrow().current.items.is_empty()).then_some(sel!(deleteConversation:)));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        if let Some(email) = self.account.email() {
            item(&email, None);
        }
        item(strings::CHAT_SIGN_OUT, Some(sel!(signOut:)));
        menu.setAutoenablesItems(false);
        menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(0.0, button.bounds().size.height + 4.0), Some(button));
    }

    // The document offered.

    fn show_document(&self) {
        let state = self.state.borrow();
        let Some(document) = &state.document else {
            self.document_chip.setHidden(true);
            return;
        };
        let name = crate::vault_window::document_name(document);
        let title = if state.document_on { format!("{name}  ✕") } else { format!("+ {name}") };
        self.document_chip.setTitle(&NSString::from_str(&title));
        self.document_chip.setAlphaValue(if state.document_on { 1.0 } else { 0.55 });
        self.document_chip.setHidden(false);
    }

    fn toggle_document(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.document_on = !state.document_on;
        }
        self.show_document();
    }

    // Asking.

    fn composer_changed(&self) {
        let empty = self.composer.string().length() == 0;
        self.placeholder.setHidden(!empty);
        let busy = self.state.borrow().turn.is_some();
        let symbol = if busy { "stop.circle.fill" } else { "arrow.up.circle.fill" };
        self.send_button.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None).as_deref());
        self.send_button.setEnabled(busy || !empty);
    }

    /// Sends the question in the field; while a turn runs, stops it.
    fn send(&self) {
        if let Some(turn) = &self.state.borrow().turn {
            turn.cancel.store(true, Ordering::Relaxed);
            return;
        }
        let text = self.composer.string().to_string().trim().to_string();
        if text.is_empty() {
            return;
        }
        if !self.account.is_signed_in() {
            return self.show_account();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let turn = {
            let mut state = self.state.borrow_mut();
            if state.model.is_empty() {
                state.model = state.current.model.clone();
            }
            state.current.model = state.model.clone();
            state.error = None;
            state.turn = Some(Asking { question: text.clone(), reply: String::new(), tools: Vec::new(), cancel: Arc::clone(&cancel) });
            let target = Arc::new(MainThreadBound::new(self.this.borrow().clone(), self.mtm()));
            let events = Arc::clone(&target);
            Turn {
                items: state.current.items.clone(),
                text,
                open: state.document.clone().filter(|_| state.document_on),
                model: state.model.clone(),
                cache_key: state.current.id.clone(),
                cancel,
                on_event: Box::new(move |event| {
                    let target = Arc::clone(&events);
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                        if let Some(panel) = target.get(mtm).upgrade() {
                            panel.chat_event(event);
                        }
                    });
                }),
                done: Box::new(move |items, result| {
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                        if let Some(panel) = target.get(mtm).upgrade() {
                            panel.turn_done(items, result);
                        }
                    });
                }),
            }
        };
        self.composer.setString(ns_string!(""));
        self.composer_changed();
        self.list_conversations();
        self.render();
        self.worker.send(turn);
    }

    fn chat_event(&self, event: ChatEvent) {
        {
            let mut state = self.state.borrow_mut();
            let Some(turn) = state.turn.as_mut() else { return };
            match event {
                ChatEvent::Text(text) => turn.reply.push_str(&text),
                ChatEvent::ToolCall { name, arguments } => turn.tools.push((name, arguments)),
                ChatEvent::ToolDone { .. } => {}
            }
        }
        self.render_later();
    }

    fn turn_done(&self, items: Vec<serde_json::Value>, result: Result<(), ChatError>) {
        let question = {
            let mut state = self.state.borrow_mut();
            let turn = state.turn.take();
            match result {
                Ok(()) => {
                    let state = &mut *state;
                    state.current.items = items;
                    state.current.name_from_first_question();
                    state.current.updated = now();
                    if let Some(conversations) = &state.conversations {
                        if let Err(e) = conversations.save(&state.current) {
                            eprintln!("knotes: cannot save the conversation: {e}");
                        }
                        state.entries = conversations.list();
                    }
                    None
                }
                Err(error) => {
                    state.error = Some(error);
                    turn.map(|t| t.question)
                }
            }
        };
        // A failed question goes back into the field, to send again.
        if let Some(question) = question
            && self.composer.string().length() == 0
        {
            self.composer.setString(&NSString::from_str(&question));
        }
        if matches!(self.state.borrow().error, Some(ChatError::SignInAgain(_))) {
            self.state.borrow_mut().sign_in_words = strings::CHAT_SIGN_IN_AGAIN.to_string();
            self.show_account();
        }
        self.composer_changed();
        self.list_conversations();
        self.render();
    }

    // The conversation shown.

    fn render_later(&self) {
        if self.render_due.replace(true) {
            return;
        }
        let target = MainThreadBound::new(self.this.borrow().clone(), self.mtm());
        let when = DispatchTime::try_from(Duration::from_millis(50)).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(panel) = target.get(mtm).upgrade() {
                panel.render();
            }
        });
    }

    fn render(&self) {
        self.render_due.set(false);
        let text = NSMutableAttributedString::new();
        let state = self.state.borrow();
        // Each block, and whether it is a tool's line (those follow each other line by line).
        let mut blocks: Vec<(Retained<NSAttributedString>, bool)> = Vec::new();
        for shown in state.current.shown() {
            blocks.push(match shown {
                Shown::User(question) => (question_text(&question), false),
                Shown::Assistant(answer) => (answer_text(&answer), false),
                Shown::Tool { name, arguments } => (tool_text(&strings::chat_tool(&name, &arguments)), true),
            });
        }
        if let Some(turn) = &state.turn {
            blocks.push((question_text(&turn.question), false));
            for (name, arguments) in &turn.tools {
                blocks.push((tool_text(&strings::chat_tool(name, arguments)), true));
            }
            blocks.push(if turn.reply.is_empty() { (tool_text(strings::CHAT_THINKING), true) } else { (answer_text(&turn.reply), false) });
        }
        if let Some(error) = &state.error {
            blocks.push((error_text(error), false));
        }
        for (i, (block, tool)) in blocks.iter().enumerate() {
            if i > 0 {
                let between = if *tool && blocks[i - 1].1 { "\n" } else { "\n\n" };
                text.appendAttributedString(&piece(between, &body_font(), &NSColor::labelColor(), None));
            }
            text.appendAttributedString(block);
        }
        // SAFETY: the transcript's own storage.
        if let Some(storage) = unsafe { self.transcript.textStorage() } {
            storage.setAttributedString(&text);
        }
        self.transcript.scrollRangeToVisible(NSRange::new(text.length(), 0));
    }

    fn follow(&self, link: &str) {
        if let Some(url) = link.strip_prefix(WEB_LINK) {
            open_url(url);
        } else if let (Some(target), Some(callback)) = (link.strip_prefix(NOTE_LINK), self.on_open_link.borrow().as_ref()) {
            callback(target.to_string());
        }
    }

    // Probe.

    pub fn probe_lines(&self) -> Vec<String> {
        let state = self.state.borrow();
        let mut lines = Vec::new();
        if self.account.is_signed_in() {
            let model = state.models.iter().find(|m| m.slug == state.model).map_or(state.model.as_str(), |m| m.name.as_str());
            let chip = if self.document_chip.isHidden() { String::new() } else { self.document_chip.title().to_string() };
            lines.push(format!("chat: signed in | model: {model} | document: {chip} | {}", strings::CHAT_PLAN));
        } else {
            lines.push(format!("chat: signed out | {} | {}", self.sign_in_button.title(), self.sign_in_status.stringValue()));
        }
        let titles: Vec<String> = (0..self.conversations_popup.numberOfItems()).map(|i| self.conversations_popup.itemTitleAtIndex(i).to_string()).collect();
        lines.push(format!("chat conversations: {} | selected {}", titles.join(", "), self.conversations_popup.indexOfSelectedItem()));
        // SAFETY: the transcript's own storage.
        let shown = unsafe { self.transcript.textStorage() }.map(|s| s.string().to_string()).unwrap_or_default();
        lines.push(format!("chat transcript: {}", shown.replace('\n', "⏎")));
        let links = self.transcript_links();
        if !links.is_empty() {
            lines.push(format!("chat links: {}", links.join(", ")));
        }
        lines.push(format!("chat field: {}", self.composer.string()));
        lines
    }

    /// The links in the conversation shown (their attribute values).
    fn transcript_links(&self) -> Vec<String> {
        // SAFETY: the transcript's own storage.
        let Some(storage) = (unsafe { self.transcript.textStorage() }) else { return Vec::new() };
        let mut links = Vec::new();
        let mut at = 0;
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            // SAFETY: an index inside the string, and a range to fill.
            let value = unsafe { storage.attribute_atIndex_effectiveRange(objc2_app_kit::NSLinkAttributeName, at, &mut range) };
            if let Some(value) = value.and_then(|v| v.downcast::<NSString>().ok()) {
                links.push(value.to_string());
            }
            at = (range.location + range.length).max(at + 1);
        }
        links
    }

    /// Probe: asks `question` as if typed and sent.
    pub fn probe_send(&self, question: &str) {
        self.composer.setString(&NSString::from_str(question));
        self.composer_changed();
        self.send();
    }

    /// Probe: clicks the first link to a note in the conversation.
    pub fn probe_click_note_link(&self) {
        if let Some(link) = self.transcript_links().into_iter().find(|l| l.starts_with(NOTE_LINK)) {
            self.follow(&link);
        }
    }

    /// Probe: the document chip clicked.
    pub fn probe_toggle_document(&self) {
        self.toggle_document();
    }

    /// Probe: a new conversation started.
    pub fn probe_new_conversation(&self) {
        self.new_conversation();
    }

    /// Probe: the list's `index`th conversation chosen.
    pub fn probe_choose_conversation(&self, index: isize) {
        self.conversations_popup.selectItemAtIndex(index);
        self.choose_conversation(index);
    }
}

/// The account: the Keychain's tokens over the network; in the probe, tokens in memory (signed
/// in with a script of answers when `KNOTES_PROBE_CHAT` names one, else signed out).
fn account() -> Account {
    if let Ok(script) = std::env::var("KNOTES_PROBE_CHAT") {
        let store = MemoryStore::default();
        let credentials = Credentials {
            client_id: "oaiapp_probe".into(),
            subject: "probe".into(),
            email: Some("probe@example.com".into()),
            id_token: String::new(),
            access_token: "probe".into(),
            refresh_token: "probe".into(),
            expires_at: u64::MAX / 2,
            scopes: vec!["chatgpt.tokens.use.direct".into()],
        };
        let _ = store.save(&credentials);
        return Account::new(Box::new(ScriptTransport::from_file(&script)), Box::new(store));
    }
    let transport: Box<dyn Transport + Send + Sync> = Box::new(UreqTransport::default());
    let store: Box<dyn CredentialStore + Send + Sync> = if crate::probe::enabled() { Box::new(MemoryStore::default()) } else { Box::new(KeychainStore) };
    Account::new(transport, store)
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn add(parent: &NSView, child: &NSView) {
    child.setTranslatesAutoresizingMaskIntoConstraints(false);
    parent.addSubview(child);
}

fn activate(constraints: &[Retained<NSLayoutConstraint>]) {
    for c in constraints {
        c.setActive(true);
    }
}

fn symbol_button(symbol: &str, label: &str, target: &ChatTarget, action: Sel, mtm: MainThreadMarker) -> Retained<NSButton> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), Some(&NSString::from_str(label))).expect("an SF Symbol");
    // SAFETY: the target outlives the button (the panel keeps both).
    let button = unsafe { NSButton::buttonWithImage_target_action(&image, Some(target), Some(action), mtm) };
    button.setBordered(false);
    if !label.is_empty() {
        button.setToolTip(Some(&NSString::from_str(label)));
    }
    button
}

fn open_url(url: &str) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

fn body_font() -> Retained<NSFont> {
    NSFont::systemFontOfSize(NSFont::systemFontSize())
}

fn any<T: ClassType<Super = NSObject> + 'static>(object: &T) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(object.retain()))
}

/// `text` in `font` and `colour`, a link to `link` when given.
fn piece(text: &str, font: &NSFont, colour: &NSColor, link: Option<&str>) -> Retained<NSAttributedString> {
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setParagraphSpacing(2.0);
    paragraph.setLineSpacing(1.5);
    let paragraph: Retained<AnyObject> = Retained::into_super(Retained::into_super(Retained::into_super(paragraph)));
    // SAFETY: AppKit's attribute names, each with a value of its kind.
    unsafe {
        let mut keys = vec![objc2_app_kit::NSFontAttributeName, objc2_app_kit::NSForegroundColorAttributeName, objc2_app_kit::NSParagraphStyleAttributeName];
        let mut values = vec![any(font), any(colour), paragraph];
        if let Some(link) = link {
            keys.push(objc2_app_kit::NSLinkAttributeName);
            values.push(any(&*NSString::from_str(link)));
        }
        let attributes = NSDictionary::from_retained_objects(&keys, &values);
        NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(text), Some(&attributes))
    }
}

fn question_text(question: &str) -> Retained<NSAttributedString> {
    // SAFETY: a font weight constant.
    let font = NSFont::systemFontOfSize_weight(NSFont::systemFontSize(), unsafe { NSFontWeightSemibold });
    piece(question, &font, &NSColor::labelColor(), None)
}

fn tool_text(words: &str) -> Retained<NSAttributedString> {
    piece(words, &NSFont::systemFontOfSize(NSFont::smallSystemFontSize()), &NSColor::tertiaryLabelColor(), None)
}

fn answer_text(markdown: &str) -> Retained<NSAttributedString> {
    let text = NSMutableAttributedString::new();
    for run in reply_runs(markdown) {
        text.appendAttributedString(&run_text(&run));
    }
    Retained::into_super(text)
}

fn run_text(run: &Run) -> Retained<NSAttributedString> {
    let size = NSFont::systemFontSize();
    let style = run.style;
    let font = if style.code || style.code_block {
        // SAFETY: a font weight constant.
        NSFont::monospacedSystemFontOfSize_weight(size - 1.0, unsafe { NSFontWeightRegular })
    } else if style.heading > 0 || style.bold {
        let grown = if style.heading > 0 { size + (4.0 - f64::from(style.heading.min(3))) } else { size };
        NSFont::boldSystemFontOfSize(grown)
    } else if style.italic {
        objc2_app_kit::NSFontManager::sharedFontManager(MainThreadMarker::new().expect("the chat is drawn on the main thread")).convertFont_toHaveTrait(&body_font(), objc2_app_kit::NSFontTraitMask::ItalicFontMask)
    } else {
        body_font()
    };
    let colour = if style.quote { NSColor::secondaryLabelColor() } else { NSColor::labelColor() };
    let link = run.link.as_ref().map(|l| match l {
        RunLink::Note(target) => format!("{NOTE_LINK}{target}"),
        RunLink::Web(url) => format!("{WEB_LINK}{url}"),
    });
    piece(&run.text, &font, &colour, link.as_deref())
}

fn error_text(error: &ChatError) -> Retained<NSAttributedString> {
    let words = match error {
        ChatError::UsageLimit => strings::CHAT_USAGE_LIMIT.to_string(),
        ChatError::Unavailable(_) => strings::CHAT_UNAVAILABLE.to_string(),
        ChatError::SignInAgain(_) => strings::CHAT_SIGN_IN_AGAIN.to_string(),
        ChatError::NotEligible(m) => format!("{}: {m}", strings::CHAT_NOT_ELIGIBLE),
        ChatError::Cancelled => strings::CHAT_STOPPED.to_string(),
        ChatError::Interrupted => strings::CHAT_CUT.to_string(),
        other => other.to_string(),
    };
    let colour = if matches!(error, ChatError::Cancelled) { NSColor::secondaryLabelColor() } else { NSColor::systemRedColor() };
    let text = NSMutableAttributedString::new();
    text.appendAttributedString(&piece(&words, &body_font(), &colour, None));
    if matches!(error, ChatError::UsageLimit) {
        text.appendAttributedString(&piece(" ", &body_font(), &colour, None));
        text.appendAttributedString(&piece(strings::CHAT_MANAGE_USAGE, &body_font(), &NSColor::linkColor(), Some(&format!("{WEB_LINK}{}", strings::CHAT_USAGE_URL))));
    }
    Retained::into_super(text)
}
