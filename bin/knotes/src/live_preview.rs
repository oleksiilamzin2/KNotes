//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::sync::Arc;

use knotes_core::{Link, LinkKind, LinkResolver};
use knotes_editor::{Canvas, Edit, FormulaCache, FormulaImage, FormulaRun, FormulaStyle, Highlight, LinkRun, ListKind, Run, Style, changed_region, highlight};

use crate::preview_fragment::{Anchor, BULLET_ROOM, Mark, PREVIEW_GAP, PreviewFragment, TASK_ROOM};
use crate::text_paint::{Paint, PaintCache, Palette, Para, STRUCK};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask,
    NSAttributedStringNSStringDrawing, NSTextContentStorage, NSTextDelegate, NSTextElement,
    NSTextElementProvider, NSTextLayoutFragment, NSTextLayoutManager, NSTextLayoutManagerDelegate, NSTextLocation, NSTextRange, NSTextStorage, NSTextStorageObserving,
    NSStrikethroughStyleAttributeName, NSTextView, NSTextViewDelegate, NSViewFrameDidChangeNotification,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSDictionary, NSInteger, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSObjectProtocol, NSRange, NSSize, NSString,
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
/// Indentation of each quote level, past its bar.
const QUOTE_INDENT: f64 = 18.0;
/// Code's inset in its background, and the height a hidden fence line keeps as padding.
const CODE_INSET: f64 = 12.0;
const FENCE_PADDING: f64 = 8.0;

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
    /// Attribute sets and paragraph styles, made once and shared.
    paint: PaintCache,
    /// Widths of a digit, `.`, `)` and a space in the digits font, once measured.
    number_widths: Cell<Option<[f64; 4]>>,
    /// Widths of characters in the body font, once measured.
    body_widths: RefCell<std::collections::HashMap<u16, f64>>,
}


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
                let edit = self.ivars().pending.get();
                let typed_space = *self.ivars().inserted.borrow() == " ";
                self.restyle_after_edit(&view);
                if let Some(edit) = edit.filter(|e| typed_space && e.removed == 0) {
                    self.complete_task_box(&view, edit.location);
                }
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
            paint: PaintCache::new(STYLE_KEY),
            number_widths: Cell::new(None),
            body_widths: RefCell::new(std::collections::HashMap::new()),
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
        // An edit not announced through shouldChangeText restyles everything.
        self.restyle(&storage, region.unwrap_or((0, total)));
    }

    fn resync(&self, storage: &NSTextStorage) {
        *self.ivars().source.borrow_mut() = storage.string().to_string();
        self.ivars().source_units.set(storage.length());
    }

    /// Restyles `region` (UTF-16), grown to whole paragraphs and to the whole of every formula
    /// meeting it. The styles are composed in memory, layer over layer, and set front to back,
    /// each run once (`text_paint`).
    fn restyle(&self, storage: &NSTextStorage, region: (usize, usize)) {
        let region = self.whole(storage, region);
        if region.0 >= region.1 {
            return;
        }
        let look = &self.ivars().look;
        let styled = self.ivars().styled.borrow();
        let fonts = self.ivars().markup_fonts.borrow();
        let revealed = self.ivars().revealed.get();
        let mut palette = Palette::new();
        let plain = palette.look(&look.body, &look.text, "plain");
        let mut canvas = Canvas::new(region.0..region.1, Paint { look: plain, para: 0, kern: 0 });
        let meets = |r: &Range<usize>| r.start < region.1 && r.end > region.0;
        for run in styled.runs.iter().filter(|r| meets(&r.range) && !matches!(r.style, Style::ListMarker { .. } | Style::QuoteMarker { .. } | Style::Fence)) {
            let (font, color, name) = self.run_look(run, &styled);
            let id = palette.look(font, color, name);
            canvas.paint(run.range.clone(), |p| p.look = id);
        }
        // After all runs (one inside would replace it), before the markup (which drops it).
        for run in styled.runs.iter().filter(|r| matches!(r.style, Style::Strike | Style::Done) && meets(&r.range)) {
            canvas.paint(run.range.clone(), |p| p.look |= STRUCK);
        }
        for (m, font) in styled.markup.iter().zip(fonts.iter()).filter(|(m, _)| meets(m)) {
            let id = if overlaps(m, revealed) { palette.look(font, &look.muted, "markup") } else { palette.look(&look.hidden, &look.clear, "hidden") };
            canvas.paint(m.clone(), |p| p.look = id);
        }
        // A done task's text is muted, bold or italic inside it too (links keep their colour).
        for run in styled.runs.iter().filter(|r| r.style == Style::Done && meets(&r.range)) {
            let mut looks: Vec<u16> = run.range.clone().filter_map(|i| canvas.get(i).map(|p| p.look)).collect();
            looks.sort_unstable();
            looks.dedup();
            let muted: Vec<(u16, u16)> = looks.into_iter().map(|id| (id, palette.recolor(id, &look.text, &look.muted))).collect();
            canvas.paint(run.range.clone(), |p| p.look = muted.iter().find(|m| m.0 == p.look).map_or(p.look, |m| m.1));
        }
        for run in styled.runs.iter().filter(|r| meets(&r.range)) {
            if let Style::QuoteMarker { depth } = run.style {
                // Shown with the cursor in its paragraph, as other markup; the paragraph indents
                // past the bars the fragment draws.
                if overlaps(&run.range, revealed) {
                    let id = palette.look(&look.body, &look.muted, "quote-marker");
                    canvas.paint(run.range.clone(), |p| p.look = id);
                } else {
                    self.paint_hidden(&mut canvas, &mut palette, storage, &run.range, "quote", 0.0);
                }
                let indent = f64::from(depth) * QUOTE_INDENT;
                // An empty quote line (`>` alone) keeps a line's height: TextKit 2 took it from
                // the hidden `>`.
                let para = palette.para(Para { first_indent: indent, head_indent: indent, min_line: self.line_height(), ..Para::default() });
                let paragraph = paragraph(storage, NSRange::new(run.range.start, 0));
                canvas.paint(paragraph.0..paragraph.1, |p| p.para = para);
            }
        }
        for block in styled.code_blocks.iter().filter(|c| meets(c)) {
            self.paint_code_block(&mut canvas, &mut palette, storage, &styled, block, overlaps(block, revealed));
        }
        for run in styled.runs.iter().filter(|r| meets(&r.range)) {
            self.paint_list_marker(&mut canvas, &mut palette, storage, run);
        }
        for f in styled.formulas.iter().filter(|f| meets(&f.range)) {
            self.paint_formula(&mut canvas, &mut palette, storage, f, overlaps(&f.range, revealed));
        }
        self.paint_formula_lines(&mut canvas, &mut palette, storage, &styled, region, revealed);
        look.paint.apply(storage, &canvas, &palette);
    }

    /// `region` grown to whole paragraphs and to the whole of every formula meeting it (a
    /// display formula styles all its paragraphs).
    fn whole(&self, storage: &NSTextStorage, region: (usize, usize)) -> (usize, usize) {
        let total = storage.length();
        let styled = self.ivars().styled.borrow();
        let mut r = (region.0.min(total), region.1.min(total));
        loop {
            let para = paragraph(storage, NSRange::new(r.0, r.1 - r.0));
            let grown = styled
                .formulas
                .iter()
                .filter(|f| f.range.start < para.1 && f.range.end > para.0)
                .fold(para, |g, f| (g.0.min(f.range.start), g.1.max(f.range.end.min(total))));
            if grown == r {
                return r;
            }
            r = grown;
        }
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
        self.restyle(&storage, (0, storage.length()));
    }

    /// Restyles what changes when the cursor moves: the markup of the paragraph it left and of
    /// the one it entered, and the list markers it now touches or left.
    fn reveal_cursor_paragraph(&self, view: &NSTextView) {
        if self.ivars().pending.get().is_some() {
            // Mid-edit: the highlight still has the old offsets; the edit restyles this itself.
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let mut regions = self.markers_touched_anew(span(view.selectedRange()));
        let new = paragraph(&storage, view.selectedRange());
        let old = self.ivars().revealed.replace(new);
        if old != new {
            regions.extend([old, new].into_iter().filter(|&p| self.reveals_in_paragraph(p)));
            // Formulas and code blocks show their source while the cursor is anywhere in them.
            let styled = self.ivars().styled.borrow();
            let blocks = styled.formulas.iter().map(|f| &f.range).chain(&styled.code_blocks);
            regions.extend(blocks.filter(|b| overlaps(b, old) != overlaps(b, new)).map(|b| (b.start, b.end)));
        }
        regions.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for r in regions {
            match merged.last_mut() {
                Some(last) if r.0 <= last.1 => last.1 = last.1.max(r.1),
                _ => merged.push(r),
            }
        }
        for region in merged {
            self.restyle(&storage, region);
        }
    }

    /// Whether a paragraph shows differently with the cursor in it: it has markup or a quote's
    /// prefix.
    fn reveals_in_paragraph(&self, para: (usize, usize)) -> bool {
        let styled = self.ivars().styled.borrow();
        let first = styled.markup.partition_point(|m| m.end <= para.0);
        let runs = styled.runs.partition_point(|r| r.range.start < para.0);
        styled.markup.get(first).is_some_and(|m| m.start < para.1)
            || styled.runs[runs..].iter().take_while(|r| r.range.start < para.1).any(|r| matches!(r.style, Style::QuoteMarker { .. }))
    }

    /// Records the new selection; the ranges of the list markers it now touches or left.
    fn markers_touched_anew(&self, new: (usize, usize)) -> Vec<(usize, usize)> {
        let old = self.ivars().selection.replace(new);
        if old == new {
            return Vec::new();
        }
        let styled = self.ivars().styled.borrow();
        // Markers are short: those touching a selection start a little before it.
        let near = |sel: (usize, usize)| {
            let first = styled.runs.partition_point(|r| r.range.start + MARKER_REACH < sel.0);
            styled.runs[first..].iter().take_while(move |r| r.range.start <= sel.1)
        };
        near(old)
            .chain(near(new))
            .filter(|r| matches!(r.style, Style::ListMarker { .. }) && touches(&r.range, old) != touches(&r.range, new))
            .map(|r| (r.range.start, r.range.end))
            .collect()
    }

    /// A list item's marker and its paragraph's indentation. A bullet's `- ` or a task's
    /// `- [ ] ` is hidden, its room kept (by kerning) for the dot or box the fragment draws,
    /// unless the selection touches it; numbers show. The source's indentation is hidden: each level indents by `LIST_INDENT`,
    /// and wrapped lines line up with the item's text.
    fn paint_list_marker<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, run: &Run) {
        let Style::ListMarker { kind, depth } = run.style else { return };
        let look = &self.ivars().look;
        let text = storage.string();
        let mut line_start = run.range.start;
        while line_start > 0 && matches!(text.characterAtIndex(line_start - 1), 0x20 | 0x09) {
            line_start -= 1;
        }
        // At a line's start, or right after a quote's `>`.
        let blank = line_start == 0 || matches!(text.characterAtIndex(line_start - 1), 0x0a | 0x0d | 0x3e);
        let hidden = kind != ListKind::Ordered && !touches(&run.range, self.ivars().selection.get());
        let font = if kind == ListKind::Ordered { &look.digits } else { &look.body };
        let hidden_room = if kind == ListKind::Bullet { BULLET_ROOM } else { TASK_ROOM };
        let room = if hidden {
            hidden_room
        } else if kind == ListKind::Ordered {
            self.number_width(&text, &run.range)
        } else {
            text_width(&text.substringWithRange(ns(&run.range)), font)
        };
        let paragraph = paragraph(storage, NSRange::new(run.range.start, 0));
        // A quote's indentation stays.
        let base = palette.para_of(canvas.get(paragraph.0).map_or(0, |p| p.para));
        let start = base.first_indent + if blank { f64::from(depth) * LIST_INDENT } else { 0.0 };
        // An empty item (its marker alone) keeps a line's height, as an empty quote line does.
        let min_line = base.min_line.max(self.line_height());
        let para = palette.para(Para { first_indent: start, head_indent: start + room, hidden_tabs: blank, min_line, ..base });
        canvas.paint(paragraph.0..paragraph.1, |p| p.para = para);
        if blank {
            let id = palette.look(&look.hidden, &look.clear, "indent");
            canvas.paint(line_start..run.range.start, |p| p.look = id);
        }
        if hidden {
            self.paint_hidden(canvas, palette, storage, &run.range, if kind == ListKind::Bullet { "bullet" } else { "task" }, hidden_room);
        } else {
            let id = palette.look(font, &look.muted, "list-marker");
            canvas.paint(run.range.clone(), |p| p.look = id);
        }
    }

    /// A code block's paragraphs inset in the background the fragments draw; its fences fold
    /// to padding unless the cursor is in the block, then they show.
    fn paint_code_block<'a>(
        &'a self,
        canvas: &mut Canvas<Paint>,
        palette: &mut Palette<'a>,
        storage: &NSTextStorage,
        styled: &Highlight,
        block: &Range<usize>,
        revealed: bool,
    ) {
        let look = &self.ivars().look;
        let paragraphs = paragraph(storage, ns(block));
        let base = palette.para_of(canvas.get(paragraphs.0).map_or(0, |p| p.para));
        let inset = base.first_indent + CODE_INSET;
        let code = palette.para(Para { first_indent: inset, head_indent: inset, ..base });
        canvas.paint(paragraphs.0..paragraphs.1, |p| p.para = code);
        let first = styled.runs.partition_point(|r| r.range.start < block.start);
        for fence in styled.runs[first..].iter().take_while(|r| r.range.start < block.end).filter(|r| r.style == Style::Fence) {
            if revealed {
                let id = palette.look(&look.mono, &look.muted, "code-fence");
                canvas.paint(fence.range.clone(), |p| p.look = id);
            } else {
                let id = palette.look(&look.hidden, &look.clear, "fence");
                canvas.paint(fence.range.clone(), |p| p.look = id);
                let folded = palette.para(Para { min_line: FENCE_PADDING, max_line: FENCE_PADDING, first_indent: inset, head_indent: inset, ..base });
                let line = paragraph(storage, NSRange::new(fence.range.start, 0));
                canvas.paint(line.0..line.1, |p| p.para = folded);
            }
        }
    }

    /// Hides the source in `range`, keeping `room` points for what the fragment draws there.
    /// The last character keeps the body font (clear): a line of tiny hidden glyphs alone sat
    /// low, and the cursor after them was drawn askew until the first key. Its width is taken
    /// back by kerning on the first character (kerning on a line's last character is dropped).
    fn paint_hidden<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, range: &Range<usize>, name: &'static str, room: f64) {
        let Some(last) = range.end.checked_sub(1).filter(|&l| l >= range.start) else { return };
        let look = &self.ivars().look;
        let width = self.body_width(storage.string().characterAtIndex(last));
        let kern = palette.kern(room - width);
        let hidden = palette.look(&look.hidden, &look.clear, name);
        canvas.paint(range.start..last, |p| {
            p.look = hidden;
            p.kern = 0;
        });
        let body = palette.look(&look.body, &look.clear, name);
        canvas.paint(last..range.end, |p| {
            p.look = body;
            p.kern = 0;
        });
        canvas.paint(range.start..range.start + 1, |p| p.kern = kern);
    }

    /// Width of a character in the body font, once measured; a tab's is next to nothing where
    /// the source's indentation is hidden.
    fn body_width(&self, unit: u16) -> f64 {
        if unit == 0x09 {
            return 0.0;
        }
        let look = &self.ivars().look;
        // Between two letters: a lone space measures as nothing (trailing white space).
        *look.body_widths.borrow_mut().entry(unit).or_insert_with(|| {
            let between = format!("x{}x", String::from_utf16_lossy(&[unit]));
            text_width(&NSString::from_str(&between), &look.body) - text_width(&NSString::from_str("xx"), &look.body)
        })
    }

    /// Height of a line of body text: what a line of hidden source alone would not get.
    fn line_height(&self) -> f64 {
        let body = &self.ivars().look.body;
        body.ascender() - body.descender() + body.leading()
    }

    /// Width of a list number (`12. `) in the digits font, without measuring the text.
    fn number_width(&self, text: &NSString, range: &Range<usize>) -> f64 {
        let look = &self.ivars().look;
        let widths = look.number_widths.get().unwrap_or_else(|| {
            let width = |s: &str| text_width(&NSString::from_str(s), &look.digits);
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
    fn paint_formula<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, f: &FormulaRun, revealed: bool) {
        let look = &self.ivars().look;
        let paragraphs = paragraph(storage, ns(&f.range));
        let block = f.display && is_block(storage, f);
        if block {
            // A display formula owns its lines: their paragraph styles are its own.
            canvas.paint(paragraphs.0..paragraphs.1, |p| p.para = 0);
        }
        let source = |canvas: &mut Canvas<Paint>, id: u16| {
            canvas.paint(f.range.clone(), |p| {
                p.look = id;
                p.kern = 0;
            })
        };
        let image = match self.formula_image(f) {
            Ok(image) => image,
            Err(_) => return source(canvas, palette.look(&look.mono, &look.error, "math-error")),
        };
        let reserved = image.height_pt + 2.0 * PREVIEW_GAP;
        if revealed {
            source(canvas, palette.look(&look.mono, &look.text, "math"));
            if block {
                // Room for the preview under the source's last line.
                let last = paragraph(storage, NSRange::new(f.range.end.saturating_sub(1), 0));
                let para = palette.para(Para { spacing_after: reserved, ..Para::default() });
                canvas.paint(last.0..last.1, |p| p.para = para);
            }
            return;
        }
        source(canvas, palette.look(&look.hidden, &look.clear, "formula"));
        if block {
            // One centred line as tall as the image; the source's other lines fold away.
            let first = paragraph(storage, NSRange::new(f.range.start, 0));
            let line = palette.para(Para { centred: true, min_line: reserved, max_line: reserved, ..Para::default() });
            canvas.paint(first.0..first.1, |p| p.para = line);
            let folded = palette.para(Para { max_line: 0.01, ..Para::default() });
            canvas.paint(first.1..paragraphs.1, |p| p.para = folded);
        }
        // On the first character: the source has spaces where lines may break, and the reserved
        // width must stay with the image (on the last character a break inside the source
        // carried it to the next line). A formula too wide for the rest of a line now moves to
        // the next one whole.
        let kern = palette.kern(image.width_pt);
        canvas.paint(f.range.start..f.range.start + 1, |p| p.kern = kern);
    }

    /// Line height of each paragraph in `region` with inline formulas: enough for the tallest
    /// typeset one above and below the baseline. Paragraphs of display formulas are left to
    /// them; the paragraph's other settings (a list item's indentation) stay.
    fn paint_formula_lines(
        &self,
        canvas: &mut Canvas<Paint>,
        palette: &mut Palette,
        storage: &NSTextStorage,
        styled: &Highlight,
        region: (usize, usize),
        revealed: (usize, usize),
    ) {
        let font = &self.ivars().look.body;
        let (ascent, descent) = (font.ascender(), -font.descender());
        let mut paragraphs: Vec<(usize, usize)> =
            styled.formulas.iter().filter(|f| f.range.start < region.1 && f.range.end > region.0).map(|f| paragraph(storage, ns(&f.range))).collect();
        paragraphs.dedup();
        for para in paragraphs {
            let in_paragraph = |f: &&FormulaRun| f.range.start < para.1 && f.range.end > para.0;
            if styled.formulas.iter().filter(in_paragraph).any(|f| f.display && is_block(storage, f)) {
                continue;
            }
            let (mut above, mut below) = (ascent, descent);
            for f in styled.formulas.iter().filter(in_paragraph).filter(|f| !overlaps(&f.range, revealed)) {
                if let Ok(image) = self.formula_image(f) {
                    above = above.max(image.baseline_pt);
                    below = below.max(image.height_pt - image.baseline_pt);
                }
            }
            let base = palette.para_of(canvas.get(para.0).map_or(0, |p| p.para));
            let min_line = above + below + 2.0;
            if (above > ascent || below > descent) && min_line > base.min_line {
                let id = palette.para(Para { min_line, ..base });
                canvas.paint(para.0..para.1, |p| p.para = id);
            }
        }
    }

    /// What the paragraph at `range` draws: each typeset formula starting in it (inline, or a
    /// display formula on its own line), the preview of a display formula with the cursor in
    /// it below its last paragraph, the dot or circle of a hidden bullet or task marker, a
    /// quote's bars and a code block's background.
    fn preview_marks(&self, layout: &NSTextLayoutManager, range: Option<&NSTextRange>) -> Vec<Mark> {
        let styled = self.ivars().styled.borrow();
        let (Some(range), false) = (range, styled.formulas.is_empty() && styled.runs.is_empty() && styled.code_blocks.is_empty()) else {
            return Vec::new();
        };
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
        let block = styled.code_blocks.partition_point(|c| c.end <= from);
        if let Some(c) = styled.code_blocks.get(block).filter(|c| c.start < to) {
            marks.push(Mark::CodeBackground { top: from <= c.start, bottom: to >= c.end });
        }
        let selection = self.ivars().selection.get();
        // A dot on the middle of small letters; a circle, as tall as capitals, on theirs.
        let centre = self.ivars().look.body.xHeight() / 2.0;
        let task_centre = self.ivars().look.body.capHeight() / 2.0;
        let first = styled.runs.partition_point(|r| r.range.start < from);
        for run in styled.runs[first..].iter().take_while(|r| r.range.start < to) {
            if let Style::QuoteMarker { depth } = run.style {
                marks.extend((0..depth).map(|level| Mark::QuoteBar { x: f64::from(level) * QUOTE_INDENT }));
                continue;
            }
            let Style::ListMarker { kind, .. } = run.style else { continue };
            if touches(&run.range, selection) {
                continue;
            }
            let offset = run.range.start - from;
            match kind {
                ListKind::Bullet => marks.push(Mark::Bullet { offset, centre }),
                ListKind::Task { checked } => marks.push(Mark::Task { offset, centre: task_centre, checked }),
                ListKind::Ordered => {}
            }
        }
        marks
    }

    /// Probe lines of what is drawn in place of hidden source in view.
    pub fn describe_drawn(view: &NSTextView) -> Vec<String> {
        crate::preview_fragment::describe_visible(view)
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
            self.restyle(&storage, (0, storage.length()));
        }
    }

    /// A click on character `index`: follows a link where its markup is hidden, or anywhere
    /// with Cmd; elsewhere the click is the text view's (ADR 0013).
    pub fn click(&self, view: &NSTextView, index: usize, cmd: bool) -> bool {
        if !cmd && self.toggle_task_at(view, index) {
            return true;
        }
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

    /// Ticks or unticks the task whose drawn box holds character `index`: an edit of the
    /// text, so it can be undone and is saved; the cursor stays.
    fn toggle_task_at(&self, view: &NSTextView, index: usize) -> bool {
        let at = {
            let styled = self.ivars().styled.borrow();
            let after = styled.runs.partition_point(|r| r.range.start <= index);
            let task = styled.runs[after.saturating_sub(MARKER_REACH)..after].iter().rev().find(|r| {
                matches!(r.style, Style::ListMarker { kind: ListKind::Task { .. }, .. }) && r.range.contains(&index) && !touches(&r.range, self.ivars().selection.get())
            });
            let Some(task) = task else { return false };
            let text = view.string();
            let Some(open) = task.range.clone().find(|&i| text.characterAtIndex(i) == u16::from(b'[')) else { return false };
            let checked = matches!(task.style, Style::ListMarker { kind: ListKind::Task { checked: true }, .. });
            (NSRange::new(open + 1, 1), if checked { " " } else { "x" })
        };
        let (range, mark) = at;
        let mark = NSString::from_str(mark);
        let selection = view.selectedRange();
        if view.shouldChangeTextInRange_replacementString(range, Some(&mark)) {
            if let Some(storage) = unsafe { view.textStorage() } {
                storage.replaceCharactersInRange_withString(range, &mark);
            }
            view.didChangeText();
            view.setSelectedRange(selection);
        }
        true
    }

    /// A space typed at `space` after a list item's `- []` makes it a task, `- [ ] `: `[]` is
    /// no task in Markdown, nor in Obsidian, but it is what one types for one. An edit of its
    /// own, so undo takes it back first.
    fn complete_task_box(&self, view: &NSTextView, space: usize) {
        let text = view.string();
        let line = text.paragraphRangeForRange(NSRange::new(space, 0)).location;
        let typed = text.substringWithRange(NSRange::new(line, space + 1 - line)).to_string();
        let Some(at) = knotes_editor::task_box_typed(&typed) else { return };
        let brackets = NSRange::new(line + typed[..at].encode_utf16().count(), 2);
        let task = NSString::from_str("[ ]");
        if view.shouldChangeTextInRange_replacementString(brackets, Some(&task)) {
            if let Some(storage) = unsafe { view.textStorage() } {
                storage.replaceCharactersInRange_withString(brackets, &task);
            }
            view.didChangeText();
            view.setSelectedRange(NSRange::new(space + 2, 0));
        }
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
            self.restyle(&storage, (0, storage.length()));
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
        Style::QuoteMarker { .. } => (&look.body, &look.muted, "quote-marker"),
        Style::Fence => (&look.mono, &look.muted, "code-fence"),
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

/// Width of `text` laid out on one line in `font`.
fn text_width(text: &NSString, font: &NSFont) -> f64 {
    // SAFETY: AppKit's attribute name constant lives for the whole process.
    let attrs = NSDictionary::from_slices(&[unsafe { NSFontAttributeName }], &[font.as_ref() as &AnyObject]);
    // SAFETY: a font is the font attribute's value type.
    unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), text, Some(&attrs)) }.size().width
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
