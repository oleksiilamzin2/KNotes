//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.

use std::cell::{Cell, RefCell};
use std::ops::Range;

use knotes_editor::{Highlight, Style, highlight};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSTextDelegate, NSTextStorage,
    NSTextView, NSTextViewDelegate,
};
use objc2_foundation::{NSDictionary, NSNotification, NSObject, NSObjectProtocol, NSRange, NSString};

pub const STYLE_KEY: &str = "knotes.style";
const BODY_SIZE: f64 = 15.0;
const HEADING_SIZES: [f64; 6] = [26.0, 22.0, 19.0, 17.0, 15.0, 15.0];

struct Look {
    body: Retained<NSFont>,
    bold: Retained<NSFont>,
    italic: Retained<NSFont>,
    mono: Retained<NSFont>,
    headings: Vec<Retained<NSFont>>,
    /// Hidden markup: a near-zero font size collapses its width; clear colour hides the rest.
    hidden: Retained<NSFont>,
    text: Retained<NSColor>,
    muted: Retained<NSColor>,
    accent: Retained<NSColor>,
    clear: Retained<NSColor>,
    style_key: Retained<NSString>,
}

pub struct Ivars {
    /// UTF-16 ranges of the shown note.
    styled: RefCell<Highlight>,
    /// Font of each markup range when visible (that of the run around it).
    markup_fonts: RefCell<Vec<Retained<NSFont>>>,
    /// Paragraph whose markup is shown (UTF-16).
    revealed: Cell<(usize, usize)>,
    look: Look,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KnotesLivePreview"]
    #[ivars = Ivars]
    pub struct LivePreview;

    unsafe impl NSObjectProtocol for LivePreview {}

    unsafe impl NSTextDelegate for LivePreview {}

    unsafe impl NSTextViewDelegate for LivePreview {
        #[unsafe(method(textViewDidChangeSelection:))]
        fn selection_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                self.reveal_cursor_paragraph(&view);
            }
        }
    }
);

impl LivePreview {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let fonts = NSFontManager::sharedFontManager(mtm);
        let body = NSFont::systemFontOfSize(BODY_SIZE);
        let look = Look {
            bold: NSFont::boldSystemFontOfSize(BODY_SIZE),
            italic: fonts.convertFont_toHaveTrait(&body, NSFontTraitMask::ItalicFontMask),
            mono: NSFont::monospacedSystemFontOfSize_weight(BODY_SIZE - 1.0, 0.0),
            headings: HEADING_SIZES.iter().map(|&s| NSFont::boldSystemFontOfSize(s)).collect(),
            hidden: NSFont::systemFontOfSize(0.01),
            text: NSColor::labelColor(),
            muted: NSColor::tertiaryLabelColor(),
            accent: NSColor::linkColor(),
            clear: NSColor::clearColor(),
            style_key: NSString::from_str(STYLE_KEY),
            body,
        };
        let ivars = Ivars {
            styled: RefCell::new(Highlight::default()),
            markup_fonts: RefCell::new(Vec::new()),
            revealed: Cell::new((0, 0)),
            look,
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Shows `source` in `view` with live preview styles.
    pub fn show(&self, view: &NSTextView, source: &str) {
        view.setString(&NSString::from_str(source));
        // A note opens at its start (setString leaves the cursor at the end).
        view.setSelectedRange(NSRange::new(0, 0));
        view.scrollRangeToVisible(NSRange::new(0, 0));
        let styled = highlight(source).to_utf16(source);
        let fonts = markup_fonts(&styled, &self.ivars().look);
        *self.ivars().markup_fonts.borrow_mut() = fonts;
        *self.ivars().styled.borrow_mut() = styled;
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let para = paragraph(&storage, view.selectedRange());
        self.ivars().revealed.set(para);
        self.apply_all(&storage, para);
    }

    fn apply_all(&self, storage: &NSTextStorage, revealed: (usize, usize)) {
        let look = &self.ivars().look;
        let styled = self.ivars().styled.borrow();
        storage.beginEditing();
        let full = NSRange::new(0, storage.length());
        set(storage, full, &look.body, &look.text, "plain", look);
        for run in &styled.runs {
            let (font, color, name) = run_look(run.style, look);
            set(storage, ns(&run.range), font, color, &name, look);
        }
        let fonts = self.ivars().markup_fonts.borrow();
        for (m, font) in styled.markup.iter().zip(fonts.iter()) {
            self.style_markup(storage, m, font, overlaps(m, revealed));
        }
        storage.endEditing();
    }

    /// Hides the markup of the previously revealed paragraph and shows that of the cursor's.
    fn reveal_cursor_paragraph(&self, view: &NSTextView) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let new = paragraph(&storage, view.selectedRange());
        let old = self.ivars().revealed.replace(new);
        if old == new {
            return;
        }
        let styled = self.ivars().styled.borrow();
        let fonts = self.ivars().markup_fonts.borrow();
        storage.beginEditing();
        for para in [old, new] {
            let first = styled.markup.partition_point(|m| m.end <= para.0);
            for (m, font) in styled.markup[first..].iter().zip(&fonts[first..]).take_while(|(m, _)| m.start < para.1) {
                self.style_markup(&storage, m, font, overlaps(m, new));
            }
        }
        storage.endEditing();
    }

    fn style_markup(&self, storage: &NSTextStorage, m: &Range<usize>, font: &NSFont, visible: bool) {
        let look = &self.ivars().look;
        if visible {
            set(storage, ns(m), font, &look.muted, "markup", look);
        } else {
            set(storage, ns(m), &look.hidden, &look.clear, "hidden", look);
        }
    }

    /// `[style]text|[style]text…` of the text view's storage, for probe mode.
    pub fn describe(view: &NSTextView) -> String {
        let Some(storage) = (unsafe { view.textStorage() }) else { return String::new() };
        let key = NSString::from_str(STYLE_KEY);
        let text = storage.string().to_string();
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let mut parts = Vec::new();
        let mut at = 0;
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            let value = unsafe { storage.attribute_atIndex_effectiveRange(&key, at, &mut range) };
            let name = value.and_then(|v| v.downcast::<NSString>().ok()).map(|s| s.to_string()).unwrap_or_default();
            let end = (range.location + range.length).max(at + 1);
            let piece = String::from_utf16_lossy(&utf16[at..end]).replace('\n', "⏎");
            parts.push(format!("[{name}]{piece}"));
            at = end;
        }
        parts.join("|")
    }
}

fn run_look(style: Style, look: &Look) -> (&NSFont, &NSColor, String) {
    match style {
        Style::Heading(level) => (&look.headings[usize::from(level.clamp(1, 6)) - 1], &look.text, format!("heading{level}")),
        Style::Strong => (&look.bold, &look.text, "strong".into()),
        Style::Emphasis => (&look.italic, &look.text, "emphasis".into()),
        Style::Strike => (&look.body, &look.muted, "strike".into()),
        Style::Code => (&look.mono, &look.text, "code".into()),
        Style::CodeBlock => (&look.mono, &look.text, "codeblock".into()),
        Style::Link => (&look.body, &look.accent, "link".into()),
        Style::WikiLink => (&look.body, &look.accent, "wikilink".into()),
        Style::Tag => (&look.body, &look.accent, "tag".into()),
    }
}

/// The font each markup range takes when shown: that of the innermost run containing it.
fn markup_fonts(styled: &Highlight, look: &Look) -> Vec<Retained<NSFont>> {
    styled
        .markup
        .iter()
        .map(|m| {
            let run = styled.runs.iter().rev().find(|r| r.range.start <= m.start && m.end <= r.range.end);
            run.map_or_else(|| look.body.clone(), |r| run_look(r.style, look).0.retain())
        })
        .collect()
}

fn set(storage: &NSTextStorage, range: NSRange, font: &NSFont, color: &NSColor, name: &str, look: &Look) {
    let style = NSString::from_str(name);
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName, &*look.style_key] };
    let values: [&AnyObject; 3] = [font.as_ref(), color.as_ref(), style.as_ref()];
    let attrs = NSDictionary::from_slices(&keys, &values);
    unsafe { storage.setAttributes_range(Some(&attrs), range) };
}

fn paragraph(storage: &NSTextStorage, selection: NSRange) -> (usize, usize) {
    let p = storage.string().paragraphRangeForRange(selection);
    (p.location, p.location + p.length)
}

fn overlaps(m: &Range<usize>, para: (usize, usize)) -> bool {
    m.start < para.1 && m.end > para.0 || (m.start == para.1 && para.0 == para.1)
}

fn ns(r: &Range<usize>) -> NSRange {
    NSRange::new(r.start, r.end - r.start)
}
