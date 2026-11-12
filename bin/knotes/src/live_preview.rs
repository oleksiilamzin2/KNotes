//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.

use std::cell::{Cell, RefCell};
use std::ops::Range;

use knotes_editor::{Edit, Highlight, Style, changed_region, highlight};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSTextDelegate, NSTextStorage,
    NSTextView, NSTextViewDelegate, NSViewFrameDidChangeNotification,
};
use objc2_foundation::{NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSRange, NSSize, NSString, NSValue};

pub const STYLE_KEY: &str = "knotes.style";
/// Longest line of note text, as Obsidian's "readable line length" (700 CSS px = 700 pt).
pub const READABLE_WIDTH: f64 = 700.0;
/// Side margin when the column is narrower than the readable width.
const MIN_MARGIN: f64 = 32.0;
const TOP_MARGIN: f64 = 24.0;
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

type OnChange = Box<dyn Fn()>;

pub struct Ivars {
    /// Called after every user edit.
    on_change: RefCell<Option<OnChange>>,
    /// The edit being made and its inserted text, between `shouldChangeText` and
    /// `textDidChange`; `None` there means "unknown edit": copy and restyle everything.
    pending: Cell<Option<Edit>>,
    inserted: RefCell<String>,
    /// Rust copy of the text, kept in step by applying each edit (copying a 300 KB `NSString`
    /// costs about 1 ms, more than the rest of a keystroke).
    source: RefCell<String>,
    /// Its length in UTF-16 units, checked against the view after each edit.
    source_units: Cell<usize>,
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

    unsafe impl NSTextDelegate for LivePreview {
        #[unsafe(method(textDidChange:))]
        fn text_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                self.restyle_after_edit(&view);
            }
            if let Some(callback) = self.ivars().on_change.borrow().as_ref() {
                callback();
            }
        }
    }

    impl LivePreview {
        /// The text view was resized: keep the text at most `READABLE_WIDTH` wide, centered.
        #[unsafe(method(frameDidChange:))]
        fn frame_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                fit_readable_width(&view);
            }
        }
    }

    unsafe impl NSTextViewDelegate for LivePreview {
        /// Every edit is announced here (AppKit calls the single-range variant only when this one
        /// is missing). One range is applied to the Rust copy; several (Replace All) resync it.
        #[unsafe(method(textView:shouldChangeTextInRanges:replacementStrings:))]
        fn should_change_text(&self, _view: &NSTextView, ranges: &NSArray<NSValue>, replacements: Option<&NSArray<NSString>>) -> bool {
            let single = (ranges.count() == 1).then(|| ranges.objectAtIndex(0).get_range()).flatten();
            let edit = single.map(|range| {
                let text = replacements.filter(|r| r.count() == 1).map(|r| r.objectAtIndex(0).to_string()).unwrap_or_default();
                let edit = Edit { location: range.location, removed: range.length, inserted: text.encode_utf16().count() };
                *self.ivars().inserted.borrow_mut() = text;
                edit
            });
            self.ivars().pending.set(edit);
            true
        }

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
            on_change: RefCell::new(None),
            pending: Cell::new(None),
            inserted: RefCell::new(String::new()),
            source: RefCell::new(String::new()),
            source_units: Cell::new(0),
            styled: RefCell::new(Highlight::default()),
            markup_fonts: RefCell::new(Vec::new()),
            revealed: Cell::new((0, 0)),
            look,
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    pub fn on_change(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_change.borrow_mut() = Some(Box::new(callback));
    }

    /// Restyles after a user edit: only the paragraphs whose styles changed (ADR 0012), plus
    /// the old and new cursor paragraphs, whose markup visibility changes.
    fn restyle_after_edit(&self, view: &NSTextView) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let total = storage.length();
        let mut edit = self.ivars().pending.take();
        if let Some(e) = edit {
            e.apply(&mut self.ivars().source.borrow_mut(), &self.ivars().inserted.borrow());
            self.ivars().source_units.set((self.ivars().source_units.get() + e.inserted).saturating_sub(e.removed));
        }
        if self.ivars().source_units.get() != total {
            edit = None;
        }
        if edit.is_none() {
            self.resync(&storage);
        }
        let source = self.ivars().source.borrow();
        let new = highlight(&source).to_utf16(&source);
        drop(source);
        let revealed = paragraph(&storage, view.selectedRange());
        let region = edit.map(|edit| {
            let (start, end) = changed_region(&self.ivars().styled.borrow(), &new, edit);
            let old = self.ivars().revealed.get();
            let old = edit.move_range(&(old.0..old.1));
            let start = start.min(old.start).min(revealed.0).min(total);
            let end = end.max(old.end).max(revealed.1).min(total);
            paragraph(&storage, NSRange::new(start, end - start))
        });
        *self.ivars().markup_fonts.borrow_mut() = markup_fonts(&new, &self.ivars().look);
        *self.ivars().styled.borrow_mut() = new;
        self.ivars().revealed.set(revealed);
        match region {
            Some(region) => self.apply_region(&storage, region, revealed),
            // An edit not announced through shouldChangeText: restyle everything.
            None => self.apply_all(&storage, revealed),
        }
    }

    fn resync(&self, storage: &NSTextStorage) {
        *self.ivars().source.borrow_mut() = storage.string().to_string();
        self.ivars().source_units.set(storage.length());
    }

    fn apply_region(&self, storage: &NSTextStorage, region: (usize, usize), revealed: (usize, usize)) {
        let look = &self.ivars().look;
        let styled = self.ivars().styled.borrow();
        let fonts = self.ivars().markup_fonts.borrow();
        let clip = |r: &Range<usize>| r.start.max(region.0)..r.end.min(region.1);
        storage.beginEditing();
        set(storage, NSRange::new(region.0, region.1 - region.0), &look.body, &look.text, "plain", look);
        for run in styled.runs.iter().filter(|r| r.range.start < region.1 && r.range.end > region.0) {
            let (font, color, name) = run_look(run.style, look);
            set(storage, ns(&clip(&run.range)), font, color, &name, look);
        }
        for (m, font) in styled.markup.iter().zip(fonts.iter()).filter(|(m, _)| m.start < region.1 && m.end > region.0) {
            self.style_markup(storage, m, font, overlaps(m, revealed));
        }
        storage.endEditing();
    }

    /// Makes `view` keep its text at the readable width as it resizes.
    pub fn attach(&self, view: &NSTextView) {
        view.setPostsFrameChangedNotifications(true);
        let observer: &AnyObject = self.as_ref();
        // SAFETY: the observer outlives the view (both live as long as the window), and the
        // selector takes one notification argument.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                observer,
                sel!(frameDidChange:),
                Some(NSViewFrameDidChangeNotification),
                Some(view.as_ref()),
            );
        }
        fit_readable_width(view);
    }

    /// Shows `source` in `view` with live preview styles.
    pub fn show(&self, view: &NSTextView, source: &str) {
        view.setString(&NSString::from_str(source));
        *self.ivars().source.borrow_mut() = source.to_owned();
        self.ivars().source_units.set(view.string().length());
        // Undo steps of the previous note would edit this one at stale offsets.
        if let Some(undo) = view.undoManager() {
            undo.removeAllActions();
        }
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
        if self.ivars().pending.get().is_some() {
            // Mid-edit: the highlight still has the old offsets; the edit restyles this itself.
            return;
        }
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

    /// Restyles everything from `source` (the edit path's worst case), for the editor bench.
    pub fn restyle_all(&self, view: &NSTextView, source: &str) {
        let styled = highlight(source).to_utf16(source);
        *self.ivars().markup_fonts.borrow_mut() = markup_fonts(&styled, &self.ivars().look);
        *self.ivars().styled.borrow_mut() = styled;
        if let Some(storage) = unsafe { view.textStorage() } {
            self.apply_all(&storage, self.ivars().revealed.get());
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

/// Side insets that center a text column of at most `READABLE_WIDTH` in the view.
pub fn fit_readable_width(view: &NSTextView) {
    let width = view.frame().size.width;
    let side = ((width - READABLE_WIDTH) / 2.0).max(MIN_MARGIN);
    if view.textContainerInset().width != side {
        view.setTextContainerInset(NSSize::new(side, TOP_MARGIN));
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
    styled.markup_owners().into_iter().map(|owner| owner.map_or_else(|| look.body.clone(), |style| run_look(style, look).0.retain())).collect()
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
