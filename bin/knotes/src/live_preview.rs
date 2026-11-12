//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::sync::Arc;

use dispatch2::{DispatchQueue, MainThreadBound};

use knotes_core::{Link, LinkKind, LinkResolver};
use knotes_editor::{Edit, FormulaCache, FormulaImage, FormulaRun, FormulaStyle, Highlight, LinkRun, Run, Style, changed_region, highlight};

use crate::formula_layers::{Anchor, FormulaLayers, PREVIEW_GAP, Placement, viewport};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSForegroundColorAttributeName, NSKernAttributeName,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSTextAlignment, NSTextDelegate, NSTextStorage, NSTextView, NSTextViewDelegate,
    NSViewBoundsDidChangeNotification, NSViewFrameDidChangeNotification,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSInteger, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSObjectProtocol, NSRange, NSSize, NSString,
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
    /// Links to missing notes (ADR 0013).
    missing: Retained<NSColor>,
    /// Formulas that do not typeset (ADR 0017).
    error: Retained<NSColor>,
    clear: Retained<NSColor>,
    style_key: Retained<NSString>,
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
    /// Typeset formulas, by source, and the layers that show them (ADR 0017).
    formula_cache: RefCell<FormulaCache>,
    formula_layers: FormulaLayers,
    /// Light or dark: formulas are typeset in the text colour.
    dark: Cell<bool>,
    /// A formula placement is scheduled.
    placing: Cell<bool>,
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
        /// The note scrolled: formulas that came into view need their layers.
        #[unsafe(method(boundsDidChange:))]
        fn bounds_did_change(&self, notification: &NSNotification) {
            let view = notification
                .object()
                .and_then(|o| o.downcast::<objc2_app_kit::NSClipView>().ok())
                .and_then(|clip| clip.documentView())
                .and_then(|v| v.downcast::<NSTextView>().ok());
            if let Some(view) = view
                && !self.ivars().styled.borrow().formulas.is_empty()
            {
                self.schedule_formulas(&view);
            }
        }

        #[unsafe(method(frameDidChange:))]
        fn frame_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                fit_readable_width(&view);
                self.schedule_formulas(&view);
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
            headings: HEADING_SIZES.iter().map(|&s| NSFont::boldSystemFontOfSize(s)).collect(),
            hidden: NSFont::systemFontOfSize(0.01),
            text: NSColor::labelColor(),
            muted: NSColor::tertiaryLabelColor(),
            accent: NSColor::linkColor(),
            missing: NSColor::linkColor().colorWithAlphaComponent(0.45),
            error: NSColor::systemRedColor(),
            clear: NSColor::clearColor(),
            style_key: NSString::from_str(STYLE_KEY),
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
            formula_layers: FormulaLayers::default(),
            dark: Cell::new(false),
            placing: Cell::new(false),
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
        self.schedule_formulas(view);
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
            set(storage, ns(&clip(&run.range)), font, color, &name, look);
        }
        for (m, font) in styled.markup.iter().zip(fonts.iter()).filter(|(m, _)| m.start < region.1 && m.end > region.0) {
            self.style_markup(storage, m, font, overlaps(m, revealed));
        }
        for f in styled.formulas.iter().filter(|f| f.range.start < region.1 && f.range.end > region.0) {
            self.style_formula(storage, f, overlaps(&f.range, revealed));
        }
        drop((styled, fonts));
        self.fit_formula_lines_in(storage, region, revealed);
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
        // Scrolling brings other formulas into view.
        if let Some(scroll) = view.enclosingScrollView() {
            let clip = scroll.contentView();
            clip.setPostsBoundsChangedNotifications(true);
            // SAFETY: as above.
            unsafe {
                NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                    observer,
                    sel!(boundsDidChange:),
                    Some(NSViewBoundsDidChangeNotification),
                    Some(clip.as_ref()),
                );
            }
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
        self.ivars().dark.set(is_dark(view));
        self.apply_all(&storage, para);
        self.schedule_formulas(view);
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
            set(storage, ns(&run.range), font, color, &name, look);
        }
        let fonts = self.ivars().markup_fonts.borrow();
        for (m, font) in styled.markup.iter().zip(fonts.iter()) {
            self.style_markup(storage, m, font, overlaps(m, revealed));
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
        if touched {
            self.schedule_formulas(view);
        }
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
            let first_line_end = f.range.end.min(first.1);
            // On the first line's last character before its line break.
            let ends_line = first_line_end > 0 && storage.string().characterAtIndex(first_line_end - 1) == 0x0a;
            let kern_at = first_line_end.saturating_sub(if ends_line { 2 } else { 1 }).max(f.range.start);
            add_kern(storage, kern_at, image.width_pt);
        } else {
            add_kern(storage, f.range.end - 1, image.width_pt);
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
        let range = NSRange::new(paragraph_range.0, paragraph_range.1 - paragraph_range.0);
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        unsafe { storage.removeAttribute_range(NSParagraphStyleAttributeName, range) };
        let font = &self.ivars().look.body;
        let (ascent, descent) = (font.ascender(), -font.descender());
        let (mut above, mut below) = (ascent, descent);
        for f in styled.formulas.iter().filter(in_paragraph).filter(|f| !overlaps(&f.range, revealed)) {
            if let Ok(image) = self.formula_image(f) {
                above = above.max(image.baseline_pt);
                below = below.max(image.height_pt - image.baseline_pt);
            }
        }
        if above > ascent || below > descent {
            let line = NSMutableParagraphStyle::new();
            line.setMinimumLineHeight(above + below + 2.0);
            add_paragraph_style(storage, paragraph_range, &line);
        }
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

    /// Places the formula layers on the next turn of the run loop, once AppKit has taken in
    /// the restyle (placing at once read frames of paragraphs not laid out again yet);
    /// requests in between make one placement.
    fn schedule_formulas(&self, view: &NSTextView) {
        if self.ivars().placing.replace(true) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new((self.retain(), view.retain()), mtm);
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            let (preview, view) = target.get(mtm);
            preview.ivars().placing.set(false);
            preview.place_formulas(view);
        });
    }

    /// Puts each typeset formula's image (or a display formula's preview) at its place.
    fn place_formulas(&self, view: &NSTextView) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let styled = self.ivars().styled.borrow();
        let revealed = self.ivars().revealed.get();
        // Only formulas TextKit laid out for the visible part: placing the others would lay out
        // the whole note (~0.5 s for 1000 formulas in 300 KB); scrolling places them later.
        let Some((from, to)) = viewport(view) else { return };
        let mut placements = Vec::new();
        for f in styled.formulas.iter().filter(|f| f.range.end >= from && f.range.start <= to) {
            let Ok(image) = self.formula_image(f) else { continue };
            let block = f.display && is_block(&storage, f);
            let anchor = match (overlaps(&f.range, revealed), block) {
                (false, true) => Anchor::Line,
                (false, false) => Anchor::Inline,
                (true, true) => Anchor::Below,
                (true, false) => continue,
            };
            let index = if anchor == Anchor::Below { f.range.end.saturating_sub(1) } else { f.range.start };
            placements.push(Placement { image, index, anchor });
        }
        drop(styled);
        self.ivars().formula_layers.place(view, &placements);
    }

    /// `x y w h kind` of each formula layer, for probe mode.
    pub fn describe_formulas(&self) -> Vec<String> {
        self.ivars().formula_layers.describe()
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
        self.schedule_formulas(view);
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
    fn run_look<'a>(&'a self, run: &Run, styled: &Highlight) -> (&'a NSFont, &'a NSColor, String) {
        let look = &self.ivars().look;
        if matches!(run.style, Style::Link | Style::WikiLink)
            && let Some(links) = self.ivars().links.borrow().as_ref()
            && let Some(link) = link_at(styled, run.range.start)
            && link.link.kind != LinkKind::External
            && links.resolve(link.link.kind, &link.link.target, &self.ivars().path.borrow()).is_none()
        {
            return (&look.body, &look.missing, "missing".into());
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
        Style::Math => (&look.body, &look.text, "math".into()),
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

fn set(storage: &NSTextStorage, range: NSRange, font: &NSFont, color: &NSColor, name: &str, look: &Look) {
    let style = NSString::from_str(name);
    // SAFETY: AppKit's attribute name constants live for the whole process.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName, &*look.style_key] };
    let values: [&AnyObject; 3] = [font.as_ref(), color.as_ref(), style.as_ref()];
    let attrs = NSDictionary::from_slices(&keys, &values);
    unsafe { storage.setAttributes_range(Some(&attrs), range) };
}

/// Drops kerning and paragraph styles (set only for formulas) from `range`.
fn clear_layout(storage: &NSTextStorage, range: NSRange) {
    // SAFETY: AppKit's attribute name constants live for the whole process.
    unsafe {
        storage.removeAttribute_range(NSKernAttributeName, range);
        storage.removeAttribute_range(NSParagraphStyleAttributeName, range);
    }
}

fn add_kern(storage: &NSTextStorage, index: usize, points: f64) {
    let value = NSNumber::numberWithDouble(points);
    // SAFETY: an NSNumber is the kerning attribute's value type.
    unsafe { storage.addAttribute_value_range(NSKernAttributeName, &value, NSRange::new(index, 1)) };
}

fn add_paragraph_style(storage: &NSTextStorage, paragraphs: (usize, usize), style: &NSMutableParagraphStyle) {
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

fn overlaps(m: &Range<usize>, para: (usize, usize)) -> bool {
    m.start < para.1 && m.end > para.0 || (m.start == para.1 && para.0 == para.1)
}

fn ns(r: &Range<usize>) -> NSRange {
    NSRange::new(r.start, r.end - r.start)
}
