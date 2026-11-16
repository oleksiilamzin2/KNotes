//! Notes embedded in the open note (ADR 0034): `![[note]]` alone on its line shows that note
//! (or its section, or block) in a box, in a text view of its own lying over the room its line
//! keeps, styled by a live preview of its own, and edited there: the edit goes back into its
//! note. The views are the note view's subviews, so they scroll with it; after each layout of
//! the note they follow their boxes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use knotes_core::{Link, LinkResolver, NoteWriter, VaultReader};
use knotes_editor::splice_part;
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSButton, NSColor, NSImage, NSTextElementProvider, NSTextView};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::live_preview::LivePreview;
use crate::note_view::{self, NoteTextView};
use crate::strings;

/// Room between a box's edge and its note's text.
pub const PAD: f64 = 12.0;
/// The button that opens an embed's note, at its box's top right, and the room the text leaves
/// it.
const OPEN_SIZE: f64 = 20.0;
const OPEN_INSET: f64 = 6.0;
const OPEN_ROOM: f64 = OPEN_SIZE + OPEN_INSET;

pub struct TargetIvars {
    on_open: Box<dyn Fn()>,
}

define_class!(
    /// The target of an embed's open button.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesEmbedOpenTarget"]
    #[ivars = TargetIvars]
    pub struct OpenTarget;

    unsafe impl NSObjectProtocol for OpenTarget {}

    impl OpenTarget {
        #[unsafe(method(open:))]
        fn open(&self, _sender: Option<&AnyObject>) {
            (self.ivars().on_open)();
        }
    }
);

impl OpenTarget {
    fn new(mtm: MainThreadMarker, on_open: impl Fn() + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { on_open: Box::new(on_open) });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }
}

/// What an embed shows: its note's vault path, the note as read and the part's range in it.
pub struct Content {
    pub path: String,
    pub full: String,
    pub range: Range<usize>,
    pub subpath: Option<String>,
}

/// What an embedded note's view tells the window: an edit (its key), or the keys taken or given
/// up.
pub struct Callbacks {
    /// A link followed in an embedded note, with that note's path (links go from there).
    pub on_link: OnLink,
    pub on_edit: Rc<dyn Fn(&str)>,
    pub on_focus: OnFocus,
    /// The open button pressed: the embed's key.
    pub on_open: Rc<dyn Fn(&str)>,
    /// The cursor moved in an embed (its key): the completion list follows.
    pub on_cursor: Rc<dyn Fn(&str)>,
    /// A key command in an embed: true when the completion list took it.
    pub on_command: Rc<dyn Fn(objc2::runtime::Sel) -> bool>,
}

/// Gets a link and the path of the embedded note it is written in.
pub type OnLink = Rc<dyn Fn(&Link, &str)>;

/// Gets an embed's key and whether its view took the keys (true) or gave them up.
pub type OnFocus = Rc<dyn Fn(&str, bool)>;

struct Shown {
    key: String,
    path: String,
    /// The part shown as read or last written, the note it is in then, and its range there.
    text: String,
    full: String,
    range: Range<usize>,
    subpath: Option<String>,
    /// Edited since last written.
    dirty: bool,
    view: Retained<NoteTextView>,
    preview: Retained<LivePreview>,
    /// The button opening its note, and its target (the button keeps it weakly).
    button: Retained<NSButton>,
    _target: Retained<OpenTarget>,
    /// The text's width and height it was measured at.
    width: f64,
    height: f64,
}

impl Shown {
    /// Its views leave the note.
    fn remove(&self) {
        self.view.removeFromSuperview();
        self.button.removeFromSuperview();
    }
}

#[derive(Default)]
pub struct NoteEmbeds {
    shown: RefCell<Vec<Shown>>,
    /// The link targets the views style their links with.
    links: RefCell<Option<Arc<dyn LinkResolver>>>,
}

impl NoteEmbeds {
    /// Shows the notes `host` embeds alone on their lines. `content` gives a link's note and part
    /// (none: not a note of the vault, which stays a link); it is asked for embeds new here, and
    /// for all when `reload` (their notes changed) but those being edited. Views are kept for
    /// embeds still there, made for new ones, and go with the rest.
    pub fn sync(&self, host: &NoteTextView, host_preview: &LivePreview, links: Option<Arc<dyn LinkResolver>>, callbacks: &Callbacks, reload: bool, content: impl Fn(&Link) -> Option<Content>) {
        let mtm = MainThreadMarker::from(host);
        let new_links = match (&links, self.links.borrow().as_ref()) {
            (Some(new), Some(old)) => !Arc::ptr_eq(new, old),
            (new, old) => new.is_some() != old.is_some(),
        };
        *self.links.borrow_mut() = links.clone();
        let width = text_width(host);
        let mut old: Vec<Shown> = self.shown.take();
        let mut shown = Vec::new();
        for (key, link, _) in host_preview.embeds(&host.string()) {
            let kept = old.iter().position(|s| s.key == key).map(|i| old.swap_remove(i));
            let mut entry = match kept {
                // Being edited: what is typed stays (it goes back into its note when written).
                Some(entry) if !reload || entry.dirty || focused(&entry.view) => entry,
                kept => {
                    let Some(content) = content(&link) else {
                        if let Some(gone) = kept {
                            gone.remove();
                        }
                        continue;
                    };
                    let mut entry = kept.unwrap_or_else(|| made(mtm, host, key.clone(), callbacks));
                    let text = content.full[content.range.clone()].to_owned();
                    if entry.text != text || entry.path != content.path {
                        entry.preview.show(&entry.view, &content.path, &text);
                        entry.text = text;
                        entry.path = content.path;
                        entry.width = 0.0;
                    }
                    entry.full = content.full;
                    entry.range = content.range;
                    entry.subpath = content.subpath;
                    if let Some(links) = links.clone().filter(|_| new_links || entry.width == 0.0) {
                        entry.preview.set_links(&entry.view, links);
                    }
                    entry
                }
            };
            if entry.width != width {
                entry.height = measure(&entry.view, width);
                entry.width = width;
            }
            shown.push(entry);
        }
        for gone in old {
            gone.remove();
        }
        *self.shown.borrow_mut() = shown;
        self.keep_rooms(host, host_preview);
    }

    /// The embed `key` edited: measured again (its box grows or shrinks with it) and marked to be
    /// written.
    pub fn edited(&self, key: &str, host: &NoteTextView, host_preview: &LivePreview) {
        let grew = {
            let mut shown = self.shown.borrow_mut();
            let Some(entry) = shown.iter_mut().find(|s| s.key == key) else { return };
            entry.dirty = true;
            let height = measure(&entry.view, entry.width);
            std::mem::replace(&mut entry.height, height) != height
        };
        if grew {
            self.keep_rooms(host, host_preview);
        }
    }

    /// The embed `key` took the keys or gave them up: its markup shows or hides.
    pub fn refocus(&self, key: &str) {
        if let Some(entry) = self.shown.borrow().iter().find(|s| s.key == key) {
            entry.preview.refocus(&entry.view);
        }
    }

    /// Writes the edited embeds back into their notes: where their parts were while a note is as
    /// last read, else where they are now; one whose part is gone is left (and said).
    pub fn save(&self, files: &FsVault) {
        for entry in self.shown.borrow_mut().iter_mut().filter(|s| s.dirty) {
            let text = entry.view.string().to_string();
            let file = files.read_text(&entry.path).unwrap_or_else(|_| entry.full.clone());
            let Some((written, range)) = splice_part(&file, &entry.full, entry.range.clone(), entry.subpath.as_deref(), &text) else {
                eprintln!("knotes: {} no longer holds the embedded part; not written", entry.path);
                continue;
            };
            match files.write_note(&entry.path, &written) {
                Ok(()) => {
                    entry.full = written;
                    entry.range = range;
                    entry.text = text;
                    entry.dirty = false;
                }
                Err(e) => eprintln!("knotes: cannot save {}: {e}", entry.path),
            }
        }
    }

    /// Puts each view over its box as the note is laid out now; one whose box is not laid out
    /// hides. True when the column's width changed since the views were measured.
    pub fn position(&self, host: &NoteTextView, host_preview: &LivePreview) -> bool {
        let shown = self.shown.borrow();
        if shown.is_empty() {
            return false;
        }
        let frames: HashMap<String, NSRect> = host_preview.embed_frames(host).into_iter().collect();
        for s in shown.iter() {
            match frames.get(&s.key) {
                Some(rect) => {
                    s.view.setHidden(false);
                    s.view.setFrame(NSRect::new(NSPoint::new(rect.origin.x + PAD, rect.origin.y + PAD), NSSize::new(s.width, s.height)));
                    s.button.setHidden(false);
                    let corner = NSPoint::new(rect.origin.x + rect.size.width - OPEN_INSET - OPEN_SIZE, rect.origin.y + OPEN_INSET);
                    s.button.setFrame(NSRect::new(corner, NSSize::new(OPEN_SIZE, OPEN_SIZE)));
                }
                None => {
                    s.view.setHidden(true);
                    s.button.setHidden(true);
                }
            }
        }
        let width = text_width(host);
        shown.iter().any(|s| s.width != width)
    }

    /// Whether `path` is the note of an embed shown.
    pub fn shows(&self, path: &str) -> bool {
        self.shown.borrow().iter().any(|s| s.path == path)
    }

    /// No embeds shown: their views go.
    pub fn clear(&self) {
        for gone in self.shown.take() {
            gone.remove();
        }
    }

    /// The note and part the embed `key` shows.
    pub fn target_of(&self, key: &str) -> Option<(String, Option<String>)> {
        self.shown.borrow().iter().find(|s| s.key == key).map(|s| (s.path.clone(), s.subpath.clone()))
    }

    /// The centre of the `n`th embed's open button and a point on its note's text, in the note
    /// view's coordinates (probe mode).
    pub fn probe_points(&self, n: usize) -> Option<(NSPoint, NSPoint)> {
        let shown = self.shown.borrow();
        let s = shown.get(n)?;
        let (b, v) = (s.button.frame(), s.view.frame());
        Some((NSPoint::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0), NSPoint::new(v.origin.x + 4.0, v.origin.y + v.size.height / 2.0)))
    }

    /// A click on character `index` of the `n`th embed's text, as the mouse gives it (probe mode).
    pub fn probe_click(&self, n: usize, index: usize) {
        let entry = self.shown.borrow().get(n).map(|s| (s.view.clone(), s.preview.clone()));
        if let Some((view, preview)) = entry {
            preview.click(&view, index, false);
        }
    }

    /// Each embed's key, view and live preview, to find in (Cmd+F).
    pub fn finders(&self) -> Vec<(String, Retained<NSTextView>, Retained<LivePreview>)> {
        self.shown.borrow().iter().map(|s| (s.key.clone(), Retained::into_super(s.view.clone()), s.preview.clone())).collect()
    }

    /// The view of the embed `key` and its note's path.
    pub fn view_of(&self, key: &str) -> Option<(Retained<NSTextView>, String)> {
        self.shown.borrow().iter().find(|s| s.key == key).map(|s| (Retained::into_super(s.view.clone()), s.path.clone()))
    }

    /// The key of the `n`th embed shown (probe mode).
    pub fn key(&self, n: usize) -> Option<String> {
        self.shown.borrow().get(n).map(|s| s.key.clone())
    }

    /// The view of the `n`th embed shown (probe mode).
    pub fn view(&self, n: usize) -> Option<Retained<NoteTextView>> {
        self.shown.borrow().get(n).map(|s| s.view.clone())
    }

    /// Each embed shown, for the probe: its note, its view's frame, its text and its styles.
    pub fn describe(&self) -> Vec<String> {
        self.shown
            .borrow()
            .iter()
            .flat_map(|s| {
                let f = s.view.frame();
                let hidden = if s.view.isHidden() { " hidden" } else { "" };
                let text = s.view.string().to_string().replace('\n', "⏎");
                let b = s.button.frame();
                let selection = s.view.selectedRange();
                [
                    format!("embed-selection: {} {}", selection.location, selection.length),
                    format!("embed: {} {:.1} {:.1} {:.1} {:.1}{hidden} | {text}", s.path, f.origin.x, f.origin.y, f.size.width, f.size.height),
                    format!("embed-editor: {}", LivePreview::describe(&s.view)),
                    format!("embed-open: {:.1} {:.1} {:.1} {:.1}", b.origin.x, b.origin.y, b.size.width, b.size.height),
                ]
            })
            .collect()
    }

    /// Each box keeps the room of its view's text.
    fn keep_rooms(&self, host: &NoteTextView, host_preview: &LivePreview) {
        let heights: HashMap<String, f64> = self.shown.borrow().iter().map(|s| (s.key.clone(), s.height + 2.0 * PAD)).collect();
        host_preview.set_embed_heights(host, heights);
        self.position(host, host_preview);
    }
}

/// A view and its live preview for the embed `key`, over `host`.
fn made(mtm: MainThreadMarker, host: &NoteTextView, key: String, callbacks: &Callbacks) -> Shown {
    let view = note_view::embed_view(mtm);
    let preview = LivePreview::new(mtm);
    view.setDelegate(Some(ProtocolObject::from_ref(&*preview)));
    preview.attach_embedded(&view);
    let clicks = preview.clone();
    view.on_click(move |view, index, cmd| clicks.click(view, index, cmd));
    let hovers = preview.clone();
    view.on_hover(move |index| hovers.clickable(index));
    let (follow, from) = (callbacks.on_link.clone(), objc2::rc::Weak::new(&*preview));
    preview.on_link(move |link| {
        if let Some(from) = from.load() {
            follow(link, &from.path());
        }
    });
    let (edit, edited_key) = (callbacks.on_edit.clone(), key.clone());
    preview.on_change(move || edit(&edited_key));
    let (cursor, cursor_key) = (callbacks.on_cursor.clone(), key.clone());
    preview.on_cursor(move || cursor(&cursor_key));
    let command = callbacks.on_command.clone();
    preview.on_command(move |sel| command(sel));
    let (focus, focused_key) = (callbacks.on_focus.clone(), key.clone());
    view.on_focus(move |taken| focus(&focused_key, taken));
    let (open, open_key) = (callbacks.on_open.clone(), key.clone());
    let target = OpenTarget::new(mtm, move || open(&open_key));
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("arrow.up.right.square"), Some(&NSString::from_str(strings::OPEN_EMBED)));
    let button = match image {
        // SAFETY: the target outlives the button's use of it (both kept in the entry).
        Some(image) => unsafe { NSButton::buttonWithImage_target_action(&image, Some(&target), Some(sel!(open:)), mtm) },
        None => unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str("↗"), Some(&target), Some(sel!(open:)), mtm) },
    };
    button.setBordered(false);
    button.setContentTintColor(Some(&NSColor::tertiaryLabelColor()));
    button.setToolTip(Some(&NSString::from_str(strings::OPEN_EMBED)));
    host.addSubview(&button);
    let (look, text) = (preview.clone(), objc2::rc::Weak::new(&*view));
    view.on_appearance(move || {
        if let Some(text) = text.load() {
            look.appearance_changed(&text);
        }
    });
    host.addSubview(&view);
    Shown { key, path: String::new(), text: String::new(), full: String::new(), range: 0..0, subpath: None, dirty: false, view, preview, button, _target: target, width: 0.0, height: 0.0 }
}

/// Whether `view` has the keys.
fn focused(view: &NSTextView) -> bool {
    view.window().and_then(|w| w.firstResponder()).is_some_and(|r| std::ptr::eq(&*r as *const _ as *const u8, view as *const _ as *const u8))
}

/// The width an embedded note's text takes in `host`: its column less the box's padding and the
/// open button's room.
fn text_width(host: &NSTextView) -> f64 {
    let column = unsafe { host.textContainer() }.map_or(0.0, |c| c.size().width);
    (column - 2.0 * PAD - OPEN_ROOM).max(40.0).floor()
}

/// The height `view`'s text takes laid out `width` wide.
fn measure(view: &NoteTextView, width: f64) -> f64 {
    view.setFrameSize(NSSize::new(width, view.frame().size.height.max(1.0)));
    let Some(layout) = view.textLayoutManager() else { return 0.0 };
    if let Some(content) = layout.textContentManager() {
        layout.ensureLayoutForRange(&content.documentRange());
    }
    layout.usageBoundsForTextContainer().size.height.ceil()
}
