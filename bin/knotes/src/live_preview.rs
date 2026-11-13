//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use knotes_core::{Link, LinkKind, LinkResolver};
use knotes_editor::{Edit, FormulaCache, FormulaImage, FormulaRun, FormulaStyle, Highlight, LinkRun, ListKind, Run, Style, changed_region, highlight};

use crate::preview_fragment::{Anchor, BULLET_ROOM, Mark, PREVIEW_GAP, PreviewFragment};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSKernAttributeName,
    NSAttributedStringNSStringDrawing, NSMutableParagraphStyle, NSParagraphStyle, NSParagraphStyleAttributeName, NSTextAlignment, NSTextContentStorage, NSTextDelegate, NSTextElement,
    NSTextElementProvider, NSTextLayoutFragment, NSTextLayoutManager, NSTextLayoutManagerDelegate, NSTextLocation, NSTextRange, NSTextStorage, NSTextStorageObserving,
    NSStrikethroughStyleAttributeName, NSTextView, NSTextViewDelegate, NSUnderlineStyle, NSViewFrameDidChangeNotification,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSDictionary, NSInteger, NSMutableCopying, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSObjectProtocol, NSRange, NSSize, NSString,
    NSValue,
};

pub const STYLE_KEY: &str = "knotes.style";
/// Longest line of note text, as Obsidian's "readable line length" (700 CSS px = 700 pt).
pub const READABLE_WIDTH: f64 = 700.0;
/// Side margin when the column is narrower than the readable width.
const MIN_MARGIN: f64 = 32.0;
const TOP_MARGIN: f64 = 24.0;
const BODY_SIZE: f64 = 15.0;
const HEADING_SIZES: [f64; 6] = [26.0, 22.0, 19.0, 17.0, 15.0, 15.0];
const HEADING_NAMES: [&str; 6] = ["heading1", "heading2", "heading3", "heading4", "heading5", "heading6"];
/// Indentation of each list level (the source's own indentation is hidden).
const LIST_INDENT: f64 = 20.0;

struct Look {
    body: Retained<NSFont>,
    bold: Retained<NSFont>,
    italic: Retained<NSFont>,
    mono: Retained<NSFont>,
    /// List numbers: digits of one width, so items' text lines up.
    digits: Retained<NSFont>,
    headings: Vec<Retained<NSFont>>,
    /// Hidden markup: a near-zero font size collapses its width; clear colour hides the rest.
    hidden: Retained<NSFont>,
    text: Retained<NSColor>,
    muted: Retained<NSColor>,
    accent: Retained<NSColor>,
    /// Links to missing notes (ADR 0013).
    missing: Retained<NSColor>,
    /// Formulas that do not typeset (ADR 0017).
    error: Retained<NSColor>,
    clear: Retained<NSColor>,
    style_key: Retained<NSString>,
    /// Attribute sets by font, colour and test name: made once, not for every styled range.
    attributes: RefCell<HashMap<AttributesKey, Retained<NSDictionary<NSString, AnyObject>>>>,
    /// List items' paragraph styles by indentation, marker room and hidden indentation.
    list_styles: RefCell<HashMap<(u64, u64, bool), Retained<NSParagraphStyle>>>,
    /// Widths of a digit, `.`, `)` and a space in the digits font, once measured.
    number_widths: Cell<Option<[f64; 4]>>,
}

/// Font, colour, test name, kerning and paragraph style of an attribute set.
type AttributesKey = (*const NSFont, *const NSColor, &'static str, u64, *const NSParagraphStyle);

type OnChange = Box<dyn Fn()>;
type OnLink = Box<dyn Fn(&Link)>;
type OnCommand = Box<dyn Fn(Sel) -> bool>;

pub struct Ivars {
    /// Called after every user edit.
    on_change: RefCell<Option<OnChange>>,
    /// Called to follow a clicked link.
    on_link: RefCell<Option<OnLink>>,
    /// Called after the cursor moved.
    on_cursor: RefCell<Option<OnChange>>,
    /// Offered key commands (`moveDown:`, `insertNewline:`…) first; true when it took one.
    on_command: RefCell<Option<OnCommand>>,
    /// Resolves the note's links, to dim those to missing notes.
    links: RefCell<Option<Arc<dyn LinkResolver>>>,
    /// Vault path of the shown note: relative links start from its folder.
    path: RefCell<String>,
    /// Typeset formulas, by source (ADR 0017); layout fragments draw them.
    formula_cache: RefCell<FormulaCache>,
    /// Light or dark: formulas are typeset in the text colour.
    dark: Cell<bool>,
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
    /// The selection (UTF-16 start and end): a list marker it touches shows its source.
    selection: Cell<(usize, usize)>,
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

    unsafe impl NSTextLayoutManagerDelegate for LivePreview {
        /// A paragraph holding typeset formulas or list bullets gets a fragment that draws them.
        /// TextKit asks again whenever a paragraph's attributes change, and every change of what
        /// they show restyles them.
        #[unsafe(method_id(textLayoutManager:textLayoutFragmentForLocation:inTextElement:))]
        fn fragment_for(
            &self,
            layout: &NSTextLayoutManager,
            _location: &ProtocolObject<dyn NSTextLocation>,
            element: &NSTextElement,
        ) -> Retained<NSTextLayoutFragment> {
            let range = element.elementRange();
            let marks = self.preview_marks(layout, range.as_deref());
            if marks.is_empty() {
                NSTextLayoutFragment::initWithTextElement_range(NSTextLayoutFragment::alloc(), element, range.as_deref())
            } else {
                Retained::into_super(PreviewFragment::new(element, range.as_deref(), marks))
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
            if let Some(callback) = self.ivars().on_cursor.borrow().as_ref() {
                callback();
            }
        }

        /// No spelling marks in formulas: their source is not words (and hidden glyphs would
        /// still show the marks as dots).
        #[unsafe(method(textView:shouldSetSpellingState:range:))]
        fn should_set_spelling_state(&self, _view: &NSTextView, state: NSInteger, range: NSRange) -> NSInteger {
            let styled = self.ivars().styled.borrow();
            let end = range.location + range.length;
            if styled.formulas.iter().any(|f| f.range.start < end && f.range.end > range.location) { 0 } else { state }
        }

        #[unsafe(method(textView:doCommandBySelector:))]
        fn do_command(&self, _view: &NSTextView, command: Sel) -> bool {
            self.ivars().on_command.borrow().as_ref().is_some_and(|callback| callback(command))
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
            digits: NSFont::monospacedDigitSystemFontOfSize_weight(BODY_SIZE, 0.0),
            headings: HEADING_SIZES.iter().map(|&s| NSFont::boldSystemFontOfSize(s)).collect(),
            hidden: NSFont::systemFontOfSize(0.01),
            text: NSColor::labelColor(),
            muted: NSColor::tertiaryLabelColor(),
            accent: NSColor::linkColor(),
            missing: NSColor::linkColor().colorWithAlphaComponent(0.45),
            error: NSColor::systemRedColor(),
            clear: NSColor::clearColor(),
            style_key: NSString::from_str(STYLE_KEY),
            attributes: RefCell::new(HashMap::new()),
            list_styles: RefCell::new(HashMap::new()),
            number_widths: Cell::new(None),
            body,
        };
        let ivars = Ivars {
            on_change: RefCell::new(None),
            on_link: RefCell::new(None),
            on_cursor: RefCell::new(None),
            on_command: RefCell::new(None),
            links: RefCell::new(None),
            path: RefCell::new(String::new()),
            formula_cache: RefCell::new(FormulaCache::default()),
            dark: Cell::new(false),
            pending: Cell::new(None),
            inserted: RefCell::new(String::new()),
            source: RefCell::new(String::new()),
            source_units: Cell::new(0),
            styled: RefCell::new(Highlight::default()),
            markup_fonts: RefCell::new(Vec::new()),
            revealed: Cell::new((0, 0)),
            selection: Cell::new((0, 0)),
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
        self.ivars().selection.set(span(view.selectedRange()));
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
        clear_layout(storage, NSRange::new(region.0, region.1 - region.0));
        for run in styled.runs.iter().filter(|r| r.range.start < region.1 && r.range.end > region.0) {
            let (font, color, name) = self.run_look(run, &styled);
            set(storage, ns(&clip(&run.range)), font, color, name, look);
        }
        for run in styled.runs.iter().filter(|r| r.style == Style::Strike && r.range.start < region.1 && r.range.end > region.0) {
            strike(storage, ns(&clip(&run.range)));
        }
        for (m, font) in styled.markup.iter().zip(fonts.iter()).filter(|(m, _)| m.start < region.1 && m.end > region.0) {
            self.style_markup(storage, m, font, overlaps(m, revealed));
        }
        for run in styled.runs.iter().filter(|r| r.range.start < region.1 && r.range.end > region.0) {
            self.style_list_marker(storage, run);
        }
        for f in styled.formulas.iter().filter(|f| f.range.start < region.1 && f.range.end > region.0) {
            self.style_formula(storage, f, overlaps(&f.range, revealed));
        }
        drop((styled, fonts));
        self.fit_formula_lines_in(storage, region, revealed);
        storage.endEditing();
    }

    /// Makes `view` keep its text at the readable width as it resizes, and draw formulas.
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
        // Paragraphs with typeset formulas draw them (the layout manager keeps a weak reference).
        if let Some(layout) = view.textLayoutManager() {
            layout.setDelegate(Some(ProtocolObject::from_ref(self)));
        }
    }

    /// Shows `source` in `view` with live preview styles.
    pub fn show(&self, view: &NSTextView, path: &str, source: &str) {
        *self.ivars().path.borrow_mut() = path.to_owned();
        // setString moves the selection, which reveals markup: the old note's ranges must not
        // be applied to the new text.
        *self.ivars().styled.borrow_mut() = Highlight::default();
        self.ivars().markup_fonts.borrow_mut().clear();
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
        self.ivars().selection.set(span(view.selectedRange()));
        self.ivars().dark.set(is_dark(view));
        self.apply_all(&storage, para);
    }

    fn apply_all(&self, storage: &NSTextStorage, revealed: (usize, usize)) {
        let look = &self.ivars().look;
        let styled = self.ivars().styled.borrow();
        storage.beginEditing();
        let full = NSRange::new(0, storage.length());
        set(storage, full, &look.body, &look.text, "plain", look);
        clear_layout(storage, full);
        for run in &styled.runs {
            let (font, color, name) = self.run_look(run, &styled);
            set(storage, ns(&run.range), font, color, name, look);
        }
        // After all runs (one inside would replace it), before the markup (which drops it).
        for run in styled.runs.iter().filter(|r| r.style == Style::Strike) {
            strike(storage, ns(&run.range));
        }
        let fonts = self.ivars().markup_fonts.borrow();
        for (m, font) in styled.markup.iter().zip(fonts.iter()) {
            self.style_markup(storage, m, font, overlaps(m, revealed));
        }
        for run in &styled.runs {
            self.style_list_marker(storage, run);
        }
        for f in &styled.formulas {
            self.style_formula(storage, f, overlaps(&f.range, revealed));
        }
        drop((styled, fonts));
        self.fit_formula_lines_in(storage, (0, storage.length()), revealed);
        storage.endEditing();
    }

    /// Hides the markup of the previously revealed paragraph and shows that of the cursor's.
    fn reveal_cursor_paragraph(&self, view: &NSTextView) {
        if self.ivars().pending.get().is_some() {
            // Mid-edit: the highlight still has the old offsets; the edit restyles this itself.
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        self.reveal_touched_markers(&storage, span(view.selectedRange()));
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
        let mut touched = false;
        for f in styled.formulas.iter().filter(|f| overlaps(&f.range, old) || overlaps(&f.range, new)) {
            self.style_formula(&storage, f, overlaps(&f.range, new));
            touched = true;
        }
        drop((fonts, styled));
        if touched {
            self.fit_formula_lines_in(&storage, (old.0.min(new.0), old.1.max(new.1)), new);
        }
        storage.endEditing();
    }

    /// Shows the source of the list markers the selection now touches and hides that of those
    /// it left.
    fn reveal_touched_markers(&self, storage: &NSTextStorage, new: (usize, usize)) {
        let old = self.ivars().selection.replace(new);
        if old == new {
            return;
        }
        let styled = self.ivars().styled.borrow();
        // Markers are short: those touching a selection start a little before it.
        let near = |sel: (usize, usize)| {
            let first = styled.runs.partition_point(|r| r.range.start + MARKER_REACH < sel.0);
            styled.runs[first..].iter().take_while(move |r| r.range.start <= sel.1)
        };
        let changed: Vec<&Run> = near(old)
            .chain(near(new))
            .filter(|r| matches!(r.style, Style::ListMarker { .. }) && touches(&r.range, old) != touches(&r.range, new))
            .collect();
        if changed.is_empty() {
            return;
        }
        let region = changed.iter().fold((usize::MAX, 0), |r, run| (r.0.min(run.range.start), r.1.max(run.range.end)));
        storage.beginEditing();
        for run in changed {
            self.style_list_marker(storage, run);
        }
        drop(styled);
        // The new paragraph style dropped a formula's line height there.
        self.fit_formula_lines_in(storage, paragraph(storage, NSRange::new(region.0, region.1 - region.0)), self.ivars().revealed.get());
        storage.endEditing();
    }

    /// A list item's marker and its paragraph's indentation. A bullet's `- ` is hidden, its room
    /// kept (by kerning) for the dot the fragment draws, unless the selection touches it;
    /// numbers show. The source's indentation is hidden: each level indents by `LIST_INDENT`,
    /// and wrapped lines line up with the item's text. One attribute change per range: the
    /// paragraph style goes on the paragraph's first character, and the text storage gives it
    /// to the whole paragraph (it fixes paragraph styles so).
    fn style_list_marker(&self, storage: &NSTextStorage, run: &Run) {
        let Style::ListMarker { kind, depth } = run.style else { return };
        let look = &self.ivars().look;
        let text = storage.string();
        let mut line_start = run.range.start;
        while line_start > 0 && matches!(text.characterAtIndex(line_start - 1), 0x20 | 0x09) {
            line_start -= 1;
        }
        let blank = line_start == 0 || matches!(text.characterAtIndex(line_start - 1), 0x0a | 0x0d);
        let range = ns(&run.range);
        let hidden = kind == ListKind::Bullet && !touches(&run.range, self.ivars().selection.get());
        let font = if kind == ListKind::Ordered { &look.digits } else { &look.body };
        let room = if hidden {
            BULLET_ROOM
        } else if kind == ListKind::Ordered {
            self.number_width(&text, &run.range)
        } else {
            let attrs = attributes(font, &look.muted, "list-marker", 0.0, None, look);
            text_width(&text.substringWithRange(range), &attrs)
        };
        let start = if blank { f64::from(depth) * LIST_INDENT } else { 0.0 };
        let style = self.list_style(start, room, blank);
        let indent = line_start..run.range.start;
        let on_marker = (!blank || indent.is_empty()).then_some(&*style);
        if hidden {
            let kern = BULLET_ROOM / run.range.len() as f64;
            set_with(storage, range, &look.hidden, &look.clear, "bullet", kern, on_marker, look);
        } else {
            set_with(storage, range, font, &look.muted, "list-marker", 0.0, on_marker, look);
        }
        if blank && !indent.is_empty() {
            set_with(storage, ns(&indent), &look.hidden, &look.clear, "indent", 0.0, Some(&style), look);
        }
    }

    /// Width of a list number (`12. `) in the digits font, without measuring the text.
    fn number_width(&self, text: &NSString, range: &Range<usize>) -> f64 {
        let look = &self.ivars().look;
        let widths = look.number_widths.get().unwrap_or_else(|| {
            let width = |s: &str| {
                let attrs = attributes(&look.digits, &look.muted, "list-marker", 0.0, None, look);
                text_width(&NSString::from_str(s), &attrs)
            };
            let w = [width("0"), width("."), width(")"), width(" ")];
            look.number_widths.set(Some(w));
            w
        });
        range
            .clone()
            .map(|i| match text.characterAtIndex(i) {
                0x30..=0x39 => widths[0],
                0x2e => widths[1],
                0x29 => widths[2],
                0x20 => widths[3],
                _ => 0.0,
            })
            .sum()
    }

    /// The paragraph style of list items starting at `start` with `room` for their marker
    /// (made once for each).
    fn list_style(&self, start: f64, room: f64, hidden_indent: bool) -> Retained<NSParagraphStyle> {
        let mut styles = self.ivars().look.list_styles.borrow_mut();
        styles
            .entry((start.to_bits(), room.to_bits(), hidden_indent))
            .or_insert_with(|| {
                let style = NSMutableParagraphStyle::new();
                style.setFirstLineHeadIndent(start);
                style.setHeadIndent(start + room);
                if hidden_indent {
                    // Hidden tabs must not jump to tab stops.
                    style.setTabStops(Some(&NSArray::new()));
                    style.setDefaultTabInterval(0.01);
                }
                Retained::into_super(style)
            })
            .clone()
    }

    /// Text colour for typeset formulas, and the typeset image of `f` (cached).
    fn formula_image(&self, f: &FormulaRun) -> Result<Arc<FormulaImage>, String> {
        let rgb = if self.ivars().dark.get() { [235, 235, 235] } else { [29, 29, 31] };
        let style = FormulaStyle { display: f.display, size_pt: BODY_SIZE + 1.0, rgb, scale: 2.0 };
        self.ivars().formula_cache.borrow_mut().get(&f.tex, &style)
    }

    /// A formula outside the cursor's paragraph: its source hidden and its room reserved
    /// (width by kerning, height by the line height) for the typeset image. With the cursor
    /// in it: its source, and room below for a display formula's preview. A formula that
    /// does not typeset shows its source marked as an error.
    fn style_formula(&self, storage: &NSTextStorage, f: &FormulaRun, revealed: bool) {
        let look = &self.ivars().look;
        let range = ns(&f.range);
        let paragraphs = paragraph(storage, range);
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        unsafe { storage.removeAttribute_range(NSKernAttributeName, range) };
        let block = f.display && is_block(storage, f);
        if block {
            // A display formula owns its lines: their paragraph styles are its own.
            // SAFETY: as above.
            unsafe { storage.removeAttribute_range(NSParagraphStyleAttributeName, NSRange::new(paragraphs.0, paragraphs.1 - paragraphs.0)) };
        }
        let image = match self.formula_image(f) {
            Ok(image) => image,
            Err(_) => {
                set(storage, range, &look.mono, &look.error, "math-error", look);
                return;
            }
        };
        if revealed {
            set(storage, range, &look.mono, &look.text, "math", look);
            if block {
                // Room for the preview under the source's last line.
                let last = paragraph(storage, NSRange::new(f.range.end.saturating_sub(1), 0));
                let style = NSMutableParagraphStyle::new();
                style.setParagraphSpacing(image.height_pt + 2.0 * PREVIEW_GAP);
                add_paragraph_style(storage, last, &style);
            }
            return;
        }
        set(storage, range, &look.hidden, &look.clear, "formula", look);
        if block {
            // One centred line as tall as the image; the source's other lines fold away.
            let first = paragraph(storage, NSRange::new(f.range.start, 0));
            let line = NSMutableParagraphStyle::new();
            line.setAlignment(NSTextAlignment::Center);
            line.setMinimumLineHeight(image.height_pt + 2.0 * PREVIEW_GAP);
            line.setMaximumLineHeight(image.height_pt + 2.0 * PREVIEW_GAP);
            add_paragraph_style(storage, first, &line);
            if first.1 < paragraphs.1 {
                let folded = NSMutableParagraphStyle::new();
                folded.setMinimumLineHeight(0.0);
                folded.setMaximumLineHeight(0.01);
                add_paragraph_style(storage, (first.1, paragraphs.1), &folded);
            }
            add_kern(storage, f.range.start, image.width_pt);
        } else {
            // On the first character: the source has spaces where lines may break, and the
            // reserved width must stay with the image (on the last character a break inside
            // the source carried it to the next line). A formula too wide for the rest of a
            // line now moves to the next one whole.
            add_kern(storage, f.range.start, image.width_pt);
        }
    }

    /// Line height of a paragraph with inline formulas: enough for the tallest typeset one
    /// above and below the baseline. Paragraphs of display formulas are left to them.
    fn fit_formula_lines(&self, storage: &NSTextStorage, paragraph_range: (usize, usize), revealed: (usize, usize)) {
        let styled = self.ivars().styled.borrow();
        let in_paragraph = |f: &&FormulaRun| f.range.start < paragraph_range.1 && f.range.end > paragraph_range.0;
        if styled.formulas.iter().filter(in_paragraph).any(|f| f.display && is_block(storage, f)) {
            return;
        }
        let font = &self.ivars().look.body;
        let (ascent, descent) = (font.ascender(), -font.descender());
        let (mut above, mut below) = (ascent, descent);
        for f in styled.formulas.iter().filter(in_paragraph).filter(|f| !overlaps(&f.range, revealed)) {
            if let Ok(image) = self.formula_image(f) {
                above = above.max(image.baseline_pt);
                below = below.max(image.height_pt - image.baseline_pt);
            }
        }
        // The paragraph's other settings (a list item's indentation) stay.
        let line = paragraph_style_at(storage, paragraph_range.0);
        line.setMinimumLineHeight(if above > ascent || below > descent { above + below + 2.0 } else { 0.0 });
        add_paragraph_style(storage, paragraph_range, &line);
    }

    /// Fits the lines of every paragraph holding an inline formula that meets `region`.
    fn fit_formula_lines_in(&self, storage: &NSTextStorage, region: (usize, usize), revealed: (usize, usize)) {
        let paragraphs: Vec<(usize, usize)> = {
            let styled = self.ivars().styled.borrow();
            let mut list: Vec<(usize, usize)> = styled
                .formulas
                .iter()
                .filter(|f| f.range.start < region.1 && f.range.end > region.0)
                .map(|f| paragraph(storage, ns(&f.range)))
                .collect();
            list.dedup();
            list
        };
        for p in paragraphs {
            self.fit_formula_lines(storage, p, revealed);
        }
    }

    /// What the paragraph at `range` draws: each typeset formula starting in it (inline, or a
    /// display formula on its own line), the preview of a display formula with the cursor in
    /// it below its last paragraph, and the dot of a hidden bullet.
    fn preview_marks(&self, layout: &NSTextLayoutManager, range: Option<&NSTextRange>) -> Vec<Mark> {
        let styled = self.ivars().styled.borrow();
        let (Some(range), false) = (range, styled.formulas.is_empty() && styled.runs.is_empty()) else { return Vec::new() };
        let Some(content) = layout.textContentManager() else { return Vec::new() };
        let storage = content.downcast_ref::<NSTextContentStorage>().and_then(|c| c.textStorage());
        let Some(storage) = storage else { return Vec::new() };
        let start = content.documentRange().location();
        let from = content.offsetFromLocation_toLocation(&start, &range.location()).max(0) as usize;
        let to = content.offsetFromLocation_toLocation(&start, &range.endLocation()).max(0) as usize;
        let revealed = self.ivars().revealed.get();
        let first = styled.formulas.partition_point(|f| f.range.end <= from);
        let mut marks = Vec::new();
        for f in styled.formulas[first..].iter().take_while(|f| f.range.start < to) {
            let block = f.display && is_block(&storage, f);
            let (anchor, at) = match (overlaps(&f.range, revealed), block) {
                (false, true) => (Anchor::Line, f.range.start),
                (false, false) => (Anchor::Inline, f.range.start),
                (true, true) => (Anchor::Below, f.range.end.saturating_sub(1)),
                (true, false) => continue,
            };
            if !(from..to).contains(&at) {
                continue;
            }
            let Ok(image) = self.formula_image(f) else { continue };
            marks.push(Mark::Formula { offset: at - from, image, anchor });
        }
        let selection = self.ivars().selection.get();
        let centre = self.ivars().look.body.xHeight() / 2.0;
        let first = styled.runs.partition_point(|r| r.range.start < from);
        for run in styled.runs[first..].iter().take_while(|r| r.range.start < to) {
            if matches!(run.style, Style::ListMarker { kind: ListKind::Bullet, .. }) && !touches(&run.range, selection) {
                marks.push(Mark::Bullet { offset: run.range.start - from, centre });
            }
        }
        marks
    }

    /// Probe lines of what is drawn in place of hidden source in view.
    pub fn describe_drawn(view: &NSTextView) -> Vec<String> {
        crate::preview_fragment::describe_visible(view)
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
    /// The shown note was renamed or moved.
    pub fn set_path(&self, path: &str) {
        *self.ivars().path.borrow_mut() = path.to_owned();
    }

    pub fn on_cursor(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_cursor.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_command(&self, callback: impl Fn(Sel) -> bool + 'static) {
        *self.ivars().on_command.borrow_mut() = Some(Box::new(callback));
    }

    pub fn on_link(&self, callback: impl Fn(&Link) + 'static) {
        *self.ivars().on_link.borrow_mut() = Some(Box::new(callback));
    }

    /// New link resolution (files came or went): restyle the note's links.
    pub fn set_links(&self, view: &NSTextView, links: Arc<dyn LinkResolver>) {
        *self.ivars().links.borrow_mut() = Some(links);
        if let Some(storage) = unsafe { view.textStorage() } {
            self.apply_all(&storage, self.ivars().revealed.get());
        }
    }

    /// A click on character `index`: follows a link where its markup is hidden, or anywhere
    /// with Cmd; elsewhere the click is the text view's (ADR 0013).
    pub fn click(&self, index: usize, cmd: bool) -> bool {
        let link = {
            let styled = self.ivars().styled.borrow();
            let revealed = self.ivars().revealed.get();
            let in_cursor_line = index >= revealed.0 && index < revealed.1;
            match link_at(&styled, index) {
                Some(link) if cmd || !in_cursor_line => link.link.clone(),
                _ => return false,
            }
        };
        if let Some(callback) = self.ivars().on_link.borrow().as_ref() {
            callback(&link);
        }
        true
    }

    /// Font, colour and test name of a run; links to missing notes are dimmed.
    fn run_look<'a>(&'a self, run: &Run, styled: &Highlight) -> (&'a NSFont, &'a NSColor, &'static str) {
        let look = &self.ivars().look;
        if matches!(run.style, Style::Link | Style::WikiLink)
            && let Some(links) = self.ivars().links.borrow().as_ref()
            && let Some(link) = link_at(styled, run.range.start)
            && link.link.kind != LinkKind::External
            && links.resolve(link.link.kind, &link.link.target, &self.ivars().path.borrow()).is_none()
        {
            return (&look.body, &look.missing, "missing");
        }
        run_look(run.style, look)
    }

    pub fn restyle_all(&self, view: &NSTextView, source: &str) {
        let styled = highlight(source).to_utf16(source);
        *self.ivars().markup_fonts.borrow_mut() = markup_fonts(&styled, &self.ivars().look);
        *self.ivars().styled.borrow_mut() = styled;
        if let Some(storage) = unsafe { view.textStorage() } {
            self.apply_all(&storage, self.ivars().revealed.get());
        }
    }

    /// The struck pieces of the text, `|` between those apart, for probe mode.
    pub fn describe_struck(view: &NSTextView) -> String {
        let Some(storage) = (unsafe { view.textStorage() }) else { return String::new() };
        let utf16: Vec<u16> = storage.string().to_string().encode_utf16().collect();
        let (mut pieces, mut at, mut last_end) = (String::new(), 0, None);
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            // SAFETY: AppKit's attribute name constant lives for the whole process.
            let value = unsafe { storage.attribute_atIndex_effectiveRange(NSStrikethroughStyleAttributeName, at, &mut range) };
            let end = (range.location + range.length).max(at + 1);
            if value.and_then(|v| v.downcast::<NSNumber>().ok()).is_some_and(|n| n.integerValue() != 0) {
                if last_end.is_some_and(|e| e != at) {
                    pieces.push('|');
                }
                pieces.push_str(&String::from_utf16_lossy(&utf16[at..end]));
                last_end = Some(end);
            }
            at = end;
        }
        pieces
    }

    /// `[style]text|[style]text…` of the text view's storage, for probe mode.
    pub fn describe(view: &NSTextView) -> String {
        let Some(storage) = (unsafe { view.textStorage() }) else { return String::new() };
        let key = NSString::from_str(STYLE_KEY);
        let text = storage.string().to_string();
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let mut parts: Vec<(String, String)> = Vec::new();
        let mut at = 0;
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            let value = unsafe { storage.attribute_atIndex_effectiveRange(&key, at, &mut range) };
            let name = value.and_then(|v| v.downcast::<NSString>().ok()).map(|s| s.to_string()).unwrap_or_default();
            let end = (range.location + range.length).max(at + 1);
            let piece = String::from_utf16_lossy(&utf16[at..end]).replace('\n', "⏎");
            // Other attributes (kerning) split effective ranges: merge runs of one style.
            match parts.last_mut() {
                Some((last, text)) if *last == name => text.push_str(&piece),
                _ => parts.push((name, piece)),
            }
            at = end;
        }
        parts.iter().map(|(name, piece)| format!("[{name}]{piece}")).collect::<Vec<_>>().join("|")
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

fn run_look(style: Style, look: &Look) -> (&NSFont, &NSColor, &'static str) {
    match style {
        Style::Heading(level) => {
            let i = usize::from(level.clamp(1, 6)) - 1;
            (&look.headings[i], &look.text, HEADING_NAMES[i])
        }
        Style::Strong => (&look.bold, &look.text, "strong"),
        Style::Emphasis => (&look.italic, &look.text, "emphasis"),
        Style::Strike => (&look.body, &look.text, "strike"),
        Style::Code => (&look.mono, &look.text, "code"),
        Style::CodeBlock => (&look.mono, &look.text, "codeblock"),
        Style::Link => (&look.body, &look.accent, "link"),
        Style::WikiLink => (&look.body, &look.accent, "wikilink"),
        Style::Tag => (&look.body, &look.accent, "tag"),
        Style::Math => (&look.body, &look.text, "math"),
        Style::ListMarker { .. } => (&look.body, &look.muted, "list-marker"),
        Style::Done => (&look.body, &look.muted, "done"),
    }
}

/// The link whose source range holds `index`.
fn link_at(styled: &Highlight, index: usize) -> Option<&LinkRun> {
    let after = styled.links.partition_point(|l| l.range.start <= index);
    styled.links[..after].last().filter(|l| index < l.range.end)
}

/// The font each markup range takes when shown: that of the innermost run containing it.
fn markup_fonts(styled: &Highlight, look: &Look) -> Vec<Retained<NSFont>> {
    styled.markup_owners().into_iter().map(|owner| owner.map_or_else(|| look.body.clone(), |style| run_look(style, look).0.retain())).collect()
}

fn set(storage: &NSTextStorage, range: NSRange, font: &NSFont, color: &NSColor, name: &'static str, look: &Look) {
    set_with(storage, range, font, color, name, 0.0, None, look);
}

/// Replaces the attributes of `range`; `kern` and `paragraph` only when given.
#[allow(clippy::too_many_arguments)]
fn set_with(
    storage: &NSTextStorage,
    range: NSRange,
    font: &NSFont,
    color: &NSColor,
    name: &'static str,
    kern: f64,
    paragraph: Option<&NSParagraphStyle>,
    look: &Look,
) {
    let attrs = attributes(font, color, name, kern, paragraph, look);
    // SAFETY: values of their attributes' types.
    unsafe { storage.setAttributes_range(Some(&attrs), range) };
}

/// An attribute set, made once and then shared.
fn attributes(
    font: &NSFont,
    color: &NSColor,
    name: &'static str,
    kern: f64,
    paragraph: Option<&NSParagraphStyle>,
    look: &Look,
) -> Retained<NSDictionary<NSString, AnyObject>> {
    let key: AttributesKey = (font, color, name, kern.to_bits(), paragraph.map_or(std::ptr::null(), |p| p as *const _));
    let mut cache = look.attributes.borrow_mut();
    cache
        .entry(key)
        .or_insert_with(|| {
            let style = NSString::from_str(name);
            let kern = NSNumber::numberWithDouble(kern);
            // SAFETY: AppKit's attribute name constants live for the whole process.
            let mut keys = unsafe { vec![NSFontAttributeName, NSForegroundColorAttributeName, &*look.style_key] };
            let mut values: Vec<&AnyObject> = vec![font.as_ref(), color.as_ref(), style.as_ref()];
            if key.3 != 0 {
                // SAFETY: as above.
                keys.push(unsafe { NSKernAttributeName });
                values.push(kern.as_ref());
            }
            if let Some(paragraph) = paragraph {
                // SAFETY: as above.
                keys.push(unsafe { NSParagraphStyleAttributeName });
                values.push(paragraph.as_ref());
            }
            NSDictionary::from_slices(&keys, &values)
        })
        .clone()
}

/// Drops kerning and paragraph styles (set only for formulas) from `range`.
fn clear_layout(storage: &NSTextStorage, range: NSRange) {
    // SAFETY: AppKit's attribute name constants live for the whole process.
    unsafe {
        storage.removeAttribute_range(NSKernAttributeName, range);
        storage.removeAttribute_range(NSParagraphStyleAttributeName, range);
    }
}

/// A line through the text of `range`.
fn strike(storage: &NSTextStorage, range: NSRange) {
    let single = NSNumber::numberWithInteger(NSUnderlineStyle::Single.0);
    // SAFETY: an NSNumber of an underline style is the attribute's value type.
    unsafe { storage.addAttribute_value_range(NSStrikethroughStyleAttributeName, &single, range) };
}

fn add_kern(storage: &NSTextStorage, index: usize, points: f64) {
    let value = NSNumber::numberWithDouble(points);
    // SAFETY: an NSNumber is the kerning attribute's value type.
    unsafe { storage.addAttribute_value_range(NSKernAttributeName, &value, NSRange::new(index, 1)) };
}

/// Width of `text` laid out on one line with `attrs`.
fn text_width(text: &NSString, attrs: &NSDictionary<NSString, AnyObject>) -> f64 {
    // SAFETY: an attribute set of valid values.
    unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), text, Some(attrs)) }.size().width
}

/// A copy of the paragraph style at `index` to change, or a new one.
fn paragraph_style_at(storage: &NSTextStorage, index: usize) -> Retained<NSMutableParagraphStyle> {
    if index < storage.length() {
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        let value = unsafe { storage.attribute_atIndex_effectiveRange(NSParagraphStyleAttributeName, index, std::ptr::null_mut()) };
        if let Some(style) = value.and_then(|v| v.downcast::<NSParagraphStyle>().ok()) {
            return style.mutableCopy();
        }
    }
    NSMutableParagraphStyle::new()
}

fn add_paragraph_style(storage: &NSTextStorage, paragraphs: (usize, usize), style: &NSParagraphStyle) {
    // SAFETY: a paragraph style is the attribute's value type.
    unsafe { storage.addAttribute_value_range(NSParagraphStyleAttributeName, style, NSRange::new(paragraphs.0, paragraphs.1 - paragraphs.0)) };
}

/// Whether a display formula fills its paragraphs (its own lines) rather than sitting in text.
fn is_block(storage: &NSTextStorage, f: &FormulaRun) -> bool {
    let (start, end) = paragraph(storage, ns(&f.range));
    let text = storage.string();
    let trailing = (f.range.end..end).all(|i| matches!(text.characterAtIndex(i), 0x0a | 0x0d | 0x20));
    start == f.range.start && trailing
}

fn is_dark(view: &NSTextView) -> bool {
    objc2_app_kit::NSAppearanceCustomization::effectiveAppearance(view).name().to_string().contains("Dark")
}

fn paragraph(storage: &NSTextStorage, selection: NSRange) -> (usize, usize) {
    let p = storage.string().paragraphRangeForRange(selection);
    (p.location, p.location + p.length)
}

/// How far before a selection a list marker touching it may start.
const MARKER_REACH: usize = 16;

/// Whether the selection touches a list marker: starts in it or ends in or after its start.
fn touches(marker: &Range<usize>, selection: (usize, usize)) -> bool {
    selection.0 < marker.end && selection.1 >= marker.start
}

fn span(selection: NSRange) -> (usize, usize) {
    (selection.location, selection.location + selection.length)
}

fn overlaps(m: &Range<usize>, para: (usize, usize)) -> bool {
    m.start < para.1 && m.end > para.0 || (m.start == para.1 && para.0 == para.1)
}

fn ns(r: &Range<usize>) -> NSRange {
    NSRange::new(r.start, r.end - r.start)
}
