//! Notes embedded in the open note (ADR 0034): `![[note]]` alone on its line shows that note
//! (or its section, or block) in a box, in a text view of its own lying over the room its line
//! keeps, styled by a live preview of its own. The views are the note view's subviews, so they
//! scroll with it; after each layout of the note they follow their boxes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use knotes_core::{Link, LinkResolver};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSTextElementProvider, NSTextView};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::live_preview::LivePreview;
use crate::note_view::{self, NoteTextView};

/// Room between a box's edge and its note's text.
pub const PAD: f64 = 12.0;

/// What an embed shows: its note's vault path and the part's text.
pub struct Content {
    pub path: String,
    pub text: String,
}

struct Shown {
    key: String,
    path: String,
    text: String,
    view: Retained<NoteTextView>,
    preview: Retained<LivePreview>,
    /// The text's width and height it was measured at.
    width: f64,
    height: f64,
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
    /// for all when `reload` (their notes changed). Views are kept for embeds still there, made
    /// for new ones, and go with the rest.
    pub fn sync(
        &self,
        host: &NoteTextView,
        host_preview: &LivePreview,
        links: Option<Arc<dyn LinkResolver>>,
        on_link: &Rc<dyn Fn(&Link)>,
        reload: bool,
        content: impl Fn(&Link) -> Option<Content>,
    ) {
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
                Some(entry) if !reload => entry,
                kept => {
                    let Some(content) = content(&link) else {
                        if let Some(gone) = kept {
                            gone.view.removeFromSuperview();
                        }
                        continue;
                    };
                    let mut entry = kept.unwrap_or_else(|| made(mtm, host, key.clone(), on_link));
                    if entry.text != content.text || entry.path != content.path {
                        entry.preview.show(&entry.view, &content.path, &content.text);
                        entry.text = content.text;
                        entry.path = content.path;
                        entry.width = 0.0;
                    }
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
            gone.view.removeFromSuperview();
        }
        let heights: HashMap<String, f64> = shown.iter().map(|s| (s.key.clone(), s.height + 2.0 * PAD)).collect();
        *self.shown.borrow_mut() = shown;
        host_preview.set_embed_heights(host, heights);
        self.position(host, host_preview);
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
                }
                None => s.view.setHidden(true),
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
            gone.view.removeFromSuperview();
        }
    }

    /// Each embed shown, for the probe: its note, its view's frame, its text and its styles.
    pub fn describe(&self) -> Vec<String> {
        self.shown
            .borrow()
            .iter()
            .flat_map(|s| {
                let f = s.view.frame();
                let hidden = if s.view.isHidden() { " hidden" } else { "" };
                [
                    format!("embed: {} {:.1} {:.1} {:.1} {:.1}{hidden} | {}", s.path, f.origin.x, f.origin.y, f.size.width, f.size.height, s.text.replace('\n', "⏎")),
                    format!("embed-editor: {}", LivePreview::describe(&s.view)),
                ]
            })
            .collect()
    }
}

/// A view and its live preview for the embed `key`, over `host`.
fn made(mtm: MainThreadMarker, host: &NoteTextView, key: String, on_link: &Rc<dyn Fn(&Link)>) -> Shown {
    let view = note_view::embed_view(mtm);
    let preview = LivePreview::new(mtm);
    view.setDelegate(Some(ProtocolObject::from_ref(&*preview)));
    preview.attach_embedded(&view);
    let clicks = preview.clone();
    view.on_click(move |view, index, cmd| clicks.click(view, index, cmd));
    let hovers = preview.clone();
    view.on_hover(move |index| hovers.clickable(index));
    let follow = on_link.clone();
    preview.on_link(move |link| follow(link));
    host.addSubview(&view);
    Shown { key, path: String::new(), text: String::new(), view, preview, width: 0.0, height: 0.0 }
}

/// The width an embedded note's text takes in `host`: its column less the box's padding.
fn text_width(host: &NSTextView) -> f64 {
    let column = unsafe { host.textContainer() }.map_or(0.0, |c| c.size().width);
    (column - 2.0 * PAD).max(40.0).floor()
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
