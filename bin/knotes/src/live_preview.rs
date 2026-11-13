//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dispatch2::DispatchQueue;

use knotes_core::{Link, LinkKind, LinkResolver};
use knotes_editor::{CalloutRun, Canvas, Edit, FormulaCache, FormulaImage, FormulaRun, FormulaStyle, Highlight, ImageRun, LinkRun, ListKind, Run, Style, TableRun, TableText, Align, changed_region, highlight, set_image_width};

use crate::image_loader::{self, DeferredImage};
use crate::preview_fragment::{Anchor, BULLET_ROOM, EXTERNAL_ROOM, Mark, PREVIEW_GAP, PreviewFragment, TASK_ROOM, TableControls, table_controls};
use crate::strings;
use crate::text_paint::{Paint, PaintCache, Palette, Para, SHADED, STRUCK};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask,
    NSAttributedStringNSStringDrawing, NSTextContentStorage, NSTextDelegate, NSTextElement,
    NSTextElementProvider, NSTextLayoutFragment, NSTextLayoutManager, NSTextLayoutManagerDelegate, NSTextLocation, NSTextRange, NSTextStorage, NSTextStorageObserving,
    NSBackgroundColorAttributeName, NSStrikethroughStyleAttributeName, NSTextView, NSTextViewDelegate, NSViewFrameDidChangeNotification,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSDictionary, NSInteger, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString,
    NSValue,
};

pub const STYLE_KEY: &str = "knotes.style";
/// Longest line of note text, as Obsidian's "readable line length" (700 CSS px = 700 pt).
pub const READABLE_WIDTH: f64 = 700.0;
/// Side margin when the column is narrower than the readable width.
const MIN_MARGIN: f64 = 32.0;
const TOP_MARGIN: f64 = 24.0;
const BODY_SIZE: f64 = 15.0;
const HEADING_SIZES: [f64; 6] = [23.0, 20.0, 18.0, 16.0, 15.0, 15.0];
const HEADING_NAMES: [&str; 6] = ["heading1", "heading2", "heading3", "heading4", "heading5", "heading6"];
/// Indentation of each list level (the source's own indentation is hidden).
const LIST_INDENT: f64 = 20.0;
/// Indentation of each quote level, past its bar.
const QUOTE_INDENT: f64 = 18.0;
/// Code's inset in its background, and the height a hidden fence line keeps as padding.
const CODE_INSET: f64 = 12.0;
const FENCE_PADDING: f64 = 8.0;
/// Narrowest an image is dragged to.
const MIN_IMAGE_WIDTH: f64 = 16.0;
/// A table cell's padding: beside its text, and above and below it.
const TABLE_PAD: f64 = 10.0;
const TABLE_VPAD: f64 = 5.0;
/// Narrowest a table column's text is.
const TABLE_MIN_COLUMN: f64 = 24.0;
/// How near a table the pointer shows its controls: beside and under it, and over it.
const TABLE_NEAR_SIDE: f64 = 26.0;
const TABLE_NEAR_TOP: f64 = 20.0;
/// Widths of pieces of table cells kept at most.
const MAX_PIECE_WIDTHS: usize = 20_000;
/// Sideways scrolling over a wide table that shows the next column.
const SCROLL_COLUMN: f64 = 30.0;

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
    /// Callout colours (ADR 0018), by `callout_look`'s index.
    callouts: Vec<Retained<NSColor>>,
    clear: Retained<NSColor>,
    /// Attribute sets and paragraph styles, made once and shared.
    paint: PaintCache,
    /// Widths of a digit, `.`, `)` and a space in the digits font, once measured.
    number_widths: Cell<Option<[f64; 4]>>,
    /// Widths of characters in the body font, once measured.
    body_widths: RefCell<std::collections::HashMap<u16, f64>>,
    /// Widths of pieces of table cells by text and font, once measured.
    piece_widths: RefCell<std::collections::HashMap<(String, usize), f64>>,
}


type OnChange = Box<dyn Fn()>;
/// A table's column widths, first column shown and start, as laid out.
type TableLayout = (Vec<f64>, usize, usize);
/// A table under the pointer: where it starts, and the row and column under the pointer.
type TableHover = (usize, Option<usize>, Option<usize>);

/// A table control (ADR 0021), with the table's index in the note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableControl {
    RowHandle(usize, usize),
    ColumnHandle(usize, usize),
    AddRow(usize),
    AddColumn(usize),
}

/// What a table control or its menu does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TableAction {
    InsertRow(usize),
    DeleteRow(usize),
    InsertColumn(usize),
    DeleteColumn(usize),
    MoveRow(usize, usize),
    MoveColumn(usize, usize),
    Align(usize, Align),
    Sort(usize, bool),
}
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
    /// The vault's folder: images are read from it (ADR 0020).
    vault: RefCell<Option<PathBuf>>,
    /// Natural size of images by vault path, once read; `None` for one that does not load.
    images: RefCell<HashMap<String, Option<NSSize>>>,
    /// Images decoded (when first drawn) for the size they show at, by vault path.
    bitmaps: RefCell<HashMap<String, Rc<DeferredImage>>>,
    /// Width of the text column, which images fit in.
    column: Cell<f64>,
    /// The image being resized (where its source starts) and the width it shows at meanwhile.
    resizing: Cell<Option<(usize, f64)>>,
    /// Each table's column edges (x from the text column's start; the right edge last), by
    /// where the table starts, as last laid out.
    table_columns: RefCell<HashMap<usize, Rc<Vec<f64>>>>,
    /// The image the pointer is over (where its line starts): it shows its resize handle.
    hovered_image: Cell<Option<usize>>,
    /// The table the pointer is over (where it starts), and the row and column under it.
    hovered_table: Cell<Option<TableHover>>,
    /// Each wide table's first column shown (by the table's index in the note): those before
    /// it are scrolled away.
    table_scroll: RefCell<HashMap<usize, usize>>,
    /// Each table's column widths with their padding (by its index), as last laid out.
    table_widths: RefCell<HashMap<usize, Vec<f64>>>,
    /// Each table's column widths, first column shown and start, as last laid out (by its
    /// index), and whether a restyle changed any: then all of its rows are set again.
    table_layouts: RefCell<HashMap<usize, TableLayout>>,
    tables_changed: Cell<bool>,
    /// Sideways scrolling over a table not yet a column's worth.
    scroll_sideways: Cell<f64>,
    /// The actions of the table menu being shown, by item tag, with the table's index.
    menu_actions: RefCell<Vec<(usize, TableAction)>>,
    /// A table edit of the live preview's own is under way: not checked against the table's
    /// structure.
    table_editing: Cell<bool>,
    /// The note's text view, to draw an image once it is decoded.
    view: RefCell<objc2::rc::Weak<NSTextView>>,
    /// Where the view's top was when far images were last let go (text container
    /// coordinates).
    images_checked_at: Cell<Option<f64>>,
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
    /// How many words the search the note was opened from found in it (highlighted).
    found: Cell<usize>,
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
        /// An item of a table's row or column menu.
        #[unsafe(method(tableMenuItem:))]
        fn table_menu_item(&self, item: &objc2_app_kit::NSMenuItem) {
            let chosen = usize::try_from(item.tag()).ok().and_then(|i| self.ivars().menu_actions.borrow().get(i).copied());
            if let (Some((t, action)), Some(view)) = (chosen, self.ivars().view.borrow().load()) {
                self.act_on_table(&view, t, action);
            }
        }

        /// The note scrolled.
        #[unsafe(method(boundsDidChange:))]
        fn bounds_did_change(&self, _notification: &NSNotification) {
            if let Some(view) = self.ivars().view.borrow().load() {
                self.let_far_images_go(&view);
            }
        }

        #[unsafe(method(frameDidChange:))]
        fn frame_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                fit_readable_width(&view);
                self.fit_images(&view);
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
        fn should_change_text(&self, view: &NSTextView, ranges: &NSArray<NSValue>, replacements: Option<&NSArray<NSString>>) -> bool {
            let single = (ranges.count() == 1).then(|| ranges.objectAtIndex(0).get_range()).flatten();
            let text = || replacements.filter(|r| r.count() == 1).map(|r| r.objectAtIndex(0).to_string()).unwrap_or_default();
            let refused = single.is_some_and(|range| !self.ivars().table_editing.get() && !self.allow_table_edit(view, range, &text()));
            if refused {
                false
            } else {
                let edit = single.map(|range| {
                    let text = text();
                    let edit = Edit { location: range.location, removed: range.length, inserted: text.encode_utf16().count() };
                    *self.ivars().inserted.borrow_mut() = text;
                    edit
                });
                self.ivars().pending.set(edit);
                true
            }
        }

        #[unsafe(method(textViewDidChangeSelection:))]
        fn selection_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                self.keep_cursor_in_cells(&view);
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
            if self.spelling_hidden(range) { 0 } else { state }
        }

        #[unsafe(method(textView:doCommandBySelector:))]
        fn do_command(&self, view: &NSTextView, command: Sel) -> bool {
            self.table_command(view, command) || self.ivars().on_command.borrow().as_ref().is_some_and(|callback| callback(command))
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
            callouts: vec![
                NSColor::systemBlueColor(),
                NSColor::systemTealColor(),
                NSColor::systemGreenColor(),
                NSColor::systemOrangeColor(),
                NSColor::systemRedColor(),
                NSColor::systemPurpleColor(),
                NSColor::systemGrayColor(),
            ],
            clear: NSColor::clearColor(),
            paint: PaintCache::new(STYLE_KEY),
            number_widths: Cell::new(None),
            body_widths: RefCell::new(std::collections::HashMap::new()),
            piece_widths: RefCell::new(std::collections::HashMap::new()),
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
            vault: RefCell::new(None),
            images: RefCell::new(HashMap::new()),
            bitmaps: RefCell::new(HashMap::new()),
            column: Cell::new(READABLE_WIDTH),
            resizing: Cell::new(None),
            table_columns: RefCell::new(HashMap::new()),
            hovered_image: Cell::new(None),
            table_editing: Cell::new(false),
            menu_actions: RefCell::new(Vec::new()),
            table_scroll: RefCell::new(HashMap::new()),
            table_widths: RefCell::new(HashMap::new()),
            scroll_sideways: Cell::new(0.0),
            table_layouts: RefCell::new(HashMap::new()),
            tables_changed: Cell::new(false),
            hovered_table: Cell::new(None),
            view: RefCell::new(objc2::rc::Weak::default()),
            images_checked_at: Cell::new(None),
            pending: Cell::new(None),
            inserted: RefCell::new(String::new()),
            source: RefCell::new(String::new()),
            source_units: Cell::new(0),
            styled: RefCell::new(Highlight::default()),
            markup_fonts: RefCell::new(Vec::new()),
            revealed: Cell::new((0, 0)),
            selection: Cell::new((0, 0)),
            found: Cell::new(0),
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
        // The found words' ranges no longer hold after an edit.
        self.clear_found(view);
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
        // A table is laid out whole (its columns are as wide as their widest cell), but its
        // rows outside the region are set again only when its columns changed: setting a row
        // lays it out again.
        let asked = self.whole(storage, region, false);
        let region = self.whole(storage, asked, true);
        if region.0 >= region.1 {
            return;
        }
        self.ivars().tables_changed.set(false);
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
        for run in styled.runs.iter().filter(|r| r.style == Style::Code && meets(&r.range)) {
            canvas.paint(run.range.clone(), |p| p.look |= SHADED);
        }
        for (m, font) in styled.markup.iter().zip(fonts.iter()).filter(|(m, _)| meets(m)) {
            let id = if overlaps(m, revealed) { palette.look(font, &look.muted, "markup") } else { palette.look(&look.hidden, &look.clear, "hidden") };
            canvas.paint(m.clone(), |p| p.look = id);
        }
        // A link to a website keeps room after its text for the arrow the fragment draws, while
        // its `](url)` is hidden.
        let arrow = palette.kern(EXTERNAL_ROOM);
        for end in external_link_ends(&styled, region, revealed) {
            canvas.paint(end.kern_at..end.kern_at + 1, |p| p.kern = arrow);
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
        for callout in styled.callouts.iter().filter(|c| meets(&c.range)) {
            self.paint_callout(&mut canvas, &mut palette, storage, callout, overlaps(&callout.marker, revealed));
        }
        for run in styled.runs.iter().filter(|r| meets(&r.range)) {
            self.paint_list_marker(&mut canvas, &mut palette, storage, run);
        }
        for f in styled.formulas.iter().filter(|f| meets(&f.range)) {
            self.paint_formula(&mut canvas, &mut palette, storage, f, overlaps(&f.range, revealed));
        }
        for image in styled.images.iter().filter(|i| meets(&i.range)) {
            self.paint_image(&mut canvas, &mut palette, storage, image, overlaps(&image.range, revealed));
        }
        for (t, table) in styled.tables.iter().enumerate().filter(|(_, t)| meets(&t.range)) {
            self.paint_table(&mut canvas, &mut palette, storage, t, table);
        }
        self.paint_formula_lines(&mut canvas, &mut palette, storage, &styled, region, revealed);
        let applied = if self.ivars().tables_changed.get() { region } else { asked };
        look.paint.apply_in(storage, &canvas, &palette, applied.0..applied.1);
    }

    /// `region` grown to whole paragraphs and to the whole of every formula meeting it (a
    /// display formula styles all its paragraphs).
    fn whole(&self, storage: &NSTextStorage, region: (usize, usize), tables: bool) -> (usize, usize) {
        let total = storage.length();
        let styled = self.ivars().styled.borrow();
        let mut r = (region.0.min(total), region.1.min(total));
        loop {
            let para = paragraph(storage, NSRange::new(r.0, r.1 - r.0));
            // A table's column widths come from all its rows.
            let grown = styled
                .formulas
                .iter()
                .map(|f| &f.range)
                .chain(styled.tables.iter().map(|t| &t.range).filter(|_| tables))
                .filter(|r| r.start < para.1 && r.end > para.0)
                .fold(para, |g, r| (g.0.min(r.start), g.1.max(r.end.min(total))));
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
        self.ivars().column.set(text_column(view));
        *self.ivars().view.borrow_mut() = objc2::rc::Weak::new(view);
        if let Some(scroll) = view.enclosingScrollView() {
            let clip = scroll.contentView();
            clip.setPostsBoundsChangedNotifications(true);
            // SAFETY: as above; the clip view lives as long as the text view.
            unsafe {
                NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                    observer,
                    sel!(boundsDidChange:),
                    Some(objc2_app_kit::NSViewBoundsDidChangeNotification),
                    Some(clip.as_ref()),
                );
            }
        }
        // Paragraphs with typeset formulas draw them (the layout manager keeps a weak reference).
        if let Some(layout) = view.textLayoutManager() {
            layout.setDelegate(Some(ProtocolObject::from_ref(self)));
        }
    }

    /// Highlights `ranges` (UTF-16), the words the search found in the note, until the next
    /// edit or note; the first is scrolled into view with the find indicator.
    pub fn show_found(&self, view: &NSTextView, ranges: &[(usize, usize)]) {
        self.highlight(view, ranges);
        let Some(&first) = ranges.first() else { return };
        let first = NSRange::new(first.0, first.1 - first.0);
        view.scrollRangeToVisible(first);
        view.showFindIndicatorForRange(first);
    }

    /// Highlights `ranges` (UTF-16) in the find colour, instead of what was highlighted, until
    /// the next edit or note.
    pub fn highlight(&self, view: &NSTextView, ranges: &[(usize, usize)]) {
        self.clear_found(view);
        let Some(layout) = view.textLayoutManager() else { return };
        let Some(content) = layout.textContentManager() else { return };
        let start = content.documentRange().location();
        let color = NSColor::findHighlightColor();
        for &(from, to) in ranges {
            let (Some(a), Some(b)) =
                (content.locationFromLocation_withOffset(&start, from as isize), content.locationFromLocation_withOffset(&start, to as isize))
            else {
                continue;
            };
            if let Some(range) = NSTextRange::initWithLocation_endLocation(NSTextRange::alloc(), &a, Some(&b)) {
                // SAFETY: a colour is the background attribute's value type.
                unsafe { layout.addRenderingAttribute_value_forTextRange(NSBackgroundColorAttributeName, Some(&color), &range) };
            }
        }
        self.ivars().found.set(ranges.len());
    }

    /// Drops the highlights of found words.
    pub fn clear_found(&self, view: &NSTextView) {
        if self.ivars().found.replace(0) == 0 {
            return;
        }
        if let (Some(layout), Some(content)) = (view.textLayoutManager(), view.textLayoutManager().and_then(|l| l.textContentManager())) {
            // SAFETY: AppKit's attribute name constant lives for the whole process.
            layout.removeRenderingAttribute_forTextRange(unsafe { NSBackgroundColorAttributeName }, &content.documentRange());
        }
    }

    /// The view's appearance changed (light or dark): formulas are typeset again in the new
    /// text colour.
    pub fn appearance_changed(&self, view: &NSTextView) {
        let dark = is_dark(view);
        if self.ivars().dark.replace(dark) != dark
            && let Some(storage) = unsafe { view.textStorage() }
        {
            self.restyle(&storage, (0, storage.length()));
        }
    }

    /// `light` or `dark`: the colour formulas are typeset in (probe mode).
    pub fn formula_ink(&self) -> &'static str {
        if self.ivars().dark.get() { "dark" } else { "light" }
    }

    /// How many found words the note shows highlighted (probe mode).
    pub fn found_count(&self) -> usize {
        self.ivars().found.get()
    }

    /// Shows `source` in `view` with live preview styles.
    pub fn show(&self, view: &NSTextView, path: &str, source: &str) {
        self.clear_found(view);
        self.ivars().hovered_image.set(None);
        // The last note's images go with it, and its tables' scrolling.
        self.forget_images();
        self.ivars().table_scroll.borrow_mut().clear();
        self.ivars().table_layouts.borrow_mut().clear();
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

    /// A callout's `[!kind]`, hidden (its room kept for the icon, and the kind's name when no
    /// title is written) unless the cursor is on its line; its title bold in its colour.
    fn paint_callout<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, callout: &CalloutRun, revealed: bool) {
        let look = &self.ivars().look;
        let (colour, _) = callout_look(&callout.kind);
        let colour = &look.callouts[colour];
        if revealed {
            let id = palette.look(&look.body, &look.muted, "callout-marker");
            canvas.paint(callout.marker.clone(), |p| p.look = id);
        } else {
            let label = if callout.title.is_empty() { text_width(&NSString::from_str(&callout_name(&callout.kind)), &look.bold) + 4.0 } else { 0.0 };
            self.paint_hidden(canvas, palette, storage, &callout.marker, "callout", CALLOUT_ICON_ROOM + label);
        }
        let title = palette.look(&look.bold, colour, "callout-title");
        canvas.paint(callout.title.clone(), |p| p.look = title);
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
    /// back by kerning on the first character (kerning on a line's last character is dropped),
    /// or on itself when there is no room to keep.
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
        // Taking width back (no room, a quote's `> `) works on the last character only: a tiny
        // glyph cannot go below no advance at all.
        let at = if room < width { last } else { range.start };
        canvas.paint(at..at + 1, |p| p.kern = kern);
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

    /// Width of `range` of `text` as `canvas` will show it: each run in its font (measured once
    /// for its text), with its kerning.
    fn shown_width(&self, canvas: &Canvas<Paint>, palette: &Palette, text: &NSString, range: &Range<usize>) -> f64 {
        let mut cache = self.ivars().look.piece_widths.borrow_mut();
        if cache.len() > MAX_PIECE_WIDTHS {
            cache.clear();
        }
        canvas
            .runs_in(range.clone())
            .map(|(run, paint)| {
                let piece = text.substringWithRange(ns(&run));
                let font = palette.font_of(paint.look);
                let key = (piece.to_string(), font as *const NSFont as usize);
                let width = *cache.entry(key).or_insert_with(|| text_width(&piece, font));
                width + palette.kern_of(paint.kern) * run.len() as f64
            })
            .sum()
    }

    /// A table as a grid (ADR 0021): each cell's text moved to its column by kerning on the
    /// hidden pipes and spaces before it, columns as wide as their widest cell, the delimiter
    /// row folded away, rows padded; the fragments draw the borders.
    fn paint_table<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, t: usize, table: &TableRun) {
        let text = storage.string();
        let columns = table.aligns.len().max(table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0));
        let mut widths: Vec<Vec<f64>> = table.rows.iter().map(|row| row.cells.iter().map(|c| self.shown_width(canvas, palette, &text, c)).collect()).collect();
        let mut column_widths = vec![TABLE_MIN_COLUMN; columns];
        for row in &widths {
            for (w, column) in row.iter().zip(column_widths.iter_mut()) {
                *column = column.max(*w);
            }
        }
        let padded: Vec<f64> = column_widths.iter().map(|w| w + 2.0 * TABLE_PAD).collect();
        let first = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0).min(columns.saturating_sub(1));
        let laid_out = (padded.clone(), first, table.range.start);
        if self.ivars().table_layouts.borrow_mut().insert(t, laid_out.clone()).as_ref() != Some(&laid_out) {
            self.ivars().tables_changed.set(true);
        }
        self.ivars().table_widths.borrow_mut().insert(t, padded);
        // Columns scrolled away collapse to the table's left edge, their text hidden.
        let hidden = palette.look(&self.ivars().look.hidden, &self.ivars().look.clear, "table");
        for (row, row_widths) in table.rows.iter().zip(widths.iter_mut()) {
            for (cell, w) in row.cells.iter().zip(row_widths.iter_mut()).take(first) {
                canvas.paint(cell.clone(), |p| {
                    p.look = hidden;
                    p.kern = 0;
                });
                *w = 0.0;
            }
        }
        let mut edges = Vec::with_capacity(columns + 1);
        let mut x = 0.0;
        for (j, w) in column_widths.iter_mut().enumerate() {
            edges.push(x);
            if j < first {
                *w = 0.0;
            } else {
                x += *w + 2.0 * TABLE_PAD;
            }
        }
        edges.push(x);
        self.ivars().table_columns.borrow_mut().insert(table.range.start, Rc::new(edges.clone()));
        for (row, widths) in table.rows.iter().zip(&widths) {
            let (mut at, mut from, mut first_indent) = (0.0, row.range.start, 0.0);
            for (j, (cell, width)) in row.cells.iter().zip(widths).enumerate() {
                let room = column_widths[j] - width;
                let offset = match table.aligns.get(j) {
                    Some(Align::Center) => room / 2.0,
                    Some(Align::Right) => room,
                    _ => 0.0,
                };
                let target = if j < first { edges[j] } else { edges[j] + TABLE_PAD + offset };
                if cell.start > from {
                    self.paint_hidden(canvas, palette, storage, &(from..cell.start), "table", target - at);
                } else {
                    // No pipe before the first cell: the line starts there.
                    first_indent = target;
                }
                at = target + width;
                from = cell.end;
            }
            if from < row.range.end {
                self.paint_hidden(canvas, palette, storage, &(from..row.range.end), "table", 0.0);
            }
            // Padding above as a taller line (spacing before the document's first paragraph is
            // dropped), below as spacing.
            let para = palette.para(Para { first_indent, unwrapped: true, min_line: self.line_height() + TABLE_VPAD, spacing_after: TABLE_VPAD, ..Para::default() });
            canvas.paint(row.range.start..row.range.end + 1, |p| p.para = para);
        }
        canvas.paint(table.delimiter.clone(), |p| {
            p.look = hidden;
            p.kern = 0;
        });
        let folded = palette.para(Para { max_line: 0.01, ..Para::default() });
        canvas.paint(table.delimiter.start..table.delimiter.end + 1, |p| p.para = folded);
    }

    /// Whether an edit of `range` to `text` may go ahead as it is: anywhere outside tables, in a
    /// cell's text, or over whole tables (ADR 0021). Inside a cell a `|` is written `\|` and a
    /// line break a space; typed between cells it goes into the nearest one; deleting a hidden
    /// pipe or space, or across cells, does nothing. Those changes are made right after.
    fn allow_table_edit(&self, view: &NSTextView, range: NSRange, text: &str) -> bool {
        let styled = self.ivars().styled.borrow();
        let (start, end) = (range.location, range.location + range.length);
        let Some(table) = styled.tables.iter().find(|t| start < t.range.end && (end > t.range.start || (start == end && start >= t.range.start))) else {
            return true;
        };
        if start <= table.range.start && end >= table.range.end {
            return true;
        }
        let in_cell = table.rows.iter().flat_map(|r| r.cells.iter()).any(|c| c.start <= start && end <= c.end);
        let clean = cell_text(text);
        if in_cell {
            if clean != text {
                self.edit_later(view, range, clean);
                return false;
            }
            return true;
        }
        if range.length == 0
            && !text.is_empty()
            && let Some(at) = nearest_cell_edge(&styled, start)
        {
            self.edit_later(view, NSRange::new(at, 0), clean);
        }
        false
    }

    /// Replaces `range` with `text` once the edit being checked is over (changing the text from
    /// inside that check would confuse the text view).
    fn edit_later(&self, view: &NSTextView, range: NSRange, text: String) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = dispatch2::MainThreadBound::new((self.retain(), view.retain()), mtm);
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            let (this, view) = target.get(mtm);
            this.ivars().table_editing.set(true);
            // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
            unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(&**view, &NSString::from_str(&text), range) };
            this.ivars().table_editing.set(false);
        });
    }

    /// Tab, Shift+Tab, Return and arrows with the cursor in a table cell (ADR 0021): to the next
    /// or the previous cell, down a row, between rows and cells; Tab in the last cell and
    /// Return in the last row add a row. False when the command is not the table's.
    fn table_command(&self, view: &NSTextView, command: Sel) -> bool {
        let selection = view.selectedRange();
        if selection.length != 0 {
            return false;
        }
        let at = selection.location;
        let styled = self.ivars().styled.borrow();
        let Some((t, row, column)) = cell_at(&styled, at) else { return false };
        let table = &styled.tables[t];
        let rows = table.rows.len();
        let cell = &table.rows[row].cells[column];
        // A row may lack cells at its end: its last one stands in.
        let end_of = |r: usize, c: usize| table.rows.get(r).and_then(|row| row.cells.get(c).or(row.cells.last())).map(|c| c.end);
        let next = if column + 1 < table.rows[row].cells.len() { Some((row, column + 1)) } else { (row + 1 < rows).then_some((row + 1, 0)) };
        let previous = if column > 0 { Some((row, column - 1)) } else { row.checked_sub(1).map(|r| (r, table.rows[r].cells.len().saturating_sub(1))) };
        let target = if command == sel!(insertTab:) {
            match next {
                Some((r, c)) => end_of(r, c),
                None => {
                    drop(styled);
                    self.rewrite_table(view, t, |table| {
                        table.insert_row(table.rows.len());
                        (table.rows.len() - 1, 0)
                    });
                    return true;
                }
            }
        } else if command == sel!(insertBacktab:) {
            previous.and_then(|(r, c)| end_of(r, c)).or(Some(at))
        } else if command == sel!(insertNewline:) {
            if row + 1 < rows {
                end_of(row + 1, column)
            } else {
                drop(styled);
                self.rewrite_table(view, t, |table| {
                    table.insert_row(table.rows.len());
                    (table.rows.len() - 1, column)
                });
                return true;
            }
        } else if command == sel!(moveDown:) && row + 1 < rows {
            end_of(row + 1, column)
        } else if command == sel!(moveUp:) && row > 0 {
            end_of(row - 1, column)
        } else if command == sel!(moveRight:) && at == cell.end && next.is_some() {
            next.and_then(|(r, c)| table.rows[r].cells.get(c)).map(|c| c.start)
        } else if command == sel!(moveLeft:) && at == cell.start && previous.is_some() {
            previous.and_then(|(r, c)| end_of(r, c))
        } else {
            return false;
        };
        drop(styled);
        if let Some(target) = target {
            view.setSelectedRange(NSRange::new(target, 0));
            view.scrollRangeToVisible(NSRange::new(target, 0));
        }
        true
    }

    /// A cursor that landed between a table's cells (a click on a border or a cell's padding, an
    /// arrow from the line above) moves into the nearest cell.
    fn keep_cursor_in_cells(&self, view: &NSTextView) {
        let selection = view.selectedRange();
        if selection.length != 0 {
            return;
        }
        let at = selection.location;
        let edge = {
            let styled = self.ivars().styled.borrow();
            if cell_at(&styled, at).is_some() { None } else { nearest_cell_edge(&styled, at) }
        };
        if let Some(edge) = edge.filter(|&e| e != at) {
            view.setSelectedRange(NSRange::new(edge, 0));
        } else {
            self.show_cursor_column(view, at);
        }
    }

    /// The pointer moved to `point` (text view coordinates), or left the view: the table it is
    /// over, or near enough for its handles and add bars, shows them.
    pub fn hover_table(&self, view: &NSTextView, point: Option<NSPoint>) {
        let hover = point.and_then(|p| self.table_at_point(view, p));
        let old = self.ivars().hovered_table.replace(hover);
        if old == hover {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let ranges: Vec<(usize, usize)> = {
            let styled = self.ivars().styled.borrow();
            [old, hover]
                .into_iter()
                .flatten()
                .filter_map(|h| styled.tables.iter().find(|t| t.range.start == h.0))
                .map(|t| (t.range.start, t.range.end.min(storage.length())))
                .collect()
        };
        for range in ranges {
            self.restyle(&storage, range);
        }
    }

    /// The table at `point` (its start) with the row and column under it, the band around it
    /// where its handles and add bars are included.
    fn table_at_point(&self, view: &NSTextView, point: NSPoint) -> Option<TableHover> {
        let layout = view.textLayoutManager()?;
        let content = layout.textContentManager()?;
        let origin = view.textContainerOrigin();
        let padding = layout.textContainer().map_or(0.0, |c| c.lineFragmentPadding());
        let styled = self.ivars().styled.borrow();
        let columns = self.ivars().table_columns.borrow();
        styled.tables.iter().find_map(|table| {
            let edges = columns.get(&table.range.start)?;
            let rows: Vec<(f64, f64)> = table.rows.iter().filter_map(|r| row_band(&layout, &content, r.range.start)).map(|(t, b)| (t + origin.y, b + origin.y)).collect();
            let (top, bottom) = (rows.first()?.0, rows.last()?.1);
            let x0 = origin.x + padding;
            let (left, right) = (x0 + edges.first()?, x0 + edges.last()?);
            let near = point.x >= left - TABLE_NEAR_SIDE && point.x <= right + TABLE_NEAR_SIDE && point.y >= top - TABLE_NEAR_TOP && point.y <= bottom + TABLE_NEAR_SIDE;
            near.then(|| {
                let row = rows.iter().position(|(t, b)| point.y >= *t && point.y < *b);
                let column = edges.windows(2).position(|w| point.x >= x0 + w[0] && point.x < x0 + w[1]);
                (table.range.start, row, column)
            })
        })
    }

    /// Sideways scrolling (`dx` points, as the wheel or trackpad gives it) with the pointer at
    /// `point`: over a table wider than the text column, it shows the next or the previous
    /// column once a column's worth has come. False when no such table is there.
    pub fn scroll_table(&self, view: &NSTextView, point: NSPoint, dx: f64) -> bool {
        let Some((start, _, _)) = self.table_at_point(view, point) else { return false };
        let Some(t) = self.ivars().styled.borrow().tables.iter().position(|t| t.range.start == start) else { return false };
        let Some(widths) = self.ivars().table_widths.borrow().get(&t).cloned() else { return false };
        let first = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0);
        let shown: f64 = widths[first.min(widths.len())..].iter().sum();
        let (more_right, more_left) = (shown > text_column(view) + 0.5, first > 0);
        if !more_right && !more_left {
            return false;
        }
        let total = self.ivars().scroll_sideways.get() + dx;
        let next = if total <= -SCROLL_COLUMN && more_right {
            Some(first + 1)
        } else if total >= SCROLL_COLUMN && more_left {
            Some(first - 1)
        } else {
            None
        };
        self.ivars().scroll_sideways.set(if next.is_some() { 0.0 } else { total });
        if let Some(next) = next {
            self.scroll_table_to(view, t, next);
        }
        true
    }

    fn scroll_table_to(&self, view: &NSTextView, t: usize, first: usize) {
        self.ivars().table_scroll.borrow_mut().insert(t, first);
        let range = self.ivars().styled.borrow().tables.get(t).map(|t| (t.range.start, t.range.end));
        if let (Some(storage), Some(range)) = (unsafe { view.textStorage() }, range) {
            self.restyle(&storage, (range.0, range.1.min(storage.length())));
        }
    }

    /// The cursor in a cell of a column scrolled away, or past the text column: the table
    /// scrolls to show it.
    fn show_cursor_column(&self, view: &NSTextView, at: usize) {
        let Some((t, _, column)) = cell_at(&self.ivars().styled.borrow(), at) else { return };
        let Some(widths) = self.ivars().table_widths.borrow().get(&t).cloned() else { return };
        let first = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0);
        let room = text_column(view);
        let mut next = first.min(column);
        while next < column && widths[next..=column].iter().sum::<f64>() > room {
            next += 1;
        }
        if next != first {
            self.scroll_table_to(view, t, next);
        }
    }

    /// The control of the table under the pointer at `point` (text view coordinates).
    pub fn table_control_at(&self, view: &NSTextView, point: NSPoint) -> Option<TableControl> {
        let (start, row, column) = self.ivars().hovered_table.get()?;
        let layout = view.textLayoutManager()?;
        let content = layout.textContentManager()?;
        let origin = view.textContainerOrigin();
        let padding = layout.textContainer().map_or(0.0, |c| c.lineFragmentPadding());
        let styled = self.ivars().styled.borrow();
        let t = styled.tables.iter().position(|t| t.range.start == start)?;
        let table = &styled.tables[t];
        let edges = self.ivars().table_columns.borrow().get(&start).cloned()?;
        let last = table.rows.len().saturating_sub(1);
        let (left, width) = (origin.x + padding + edges.first()?, edges.last()? - edges.first()?);
        // A wide table's controls stop where the text column does, as drawn: no add column bar
        // while its right edge is cut.
        let shown = width.min(text_column(view) + 2.0 * padding - edges.first()?);
        let (width, cut) = (shown, shown < width - 0.5);
        for (i, r) in table.rows.iter().enumerate() {
            let Some((top, bottom)) = row_band(&layout, &content, r.range.start) else { continue };
            let rect = NSRect::new(NSPoint::new(left, origin.y + top), NSSize::new(width, bottom - top));
            let controls = TableControls { handle: row == Some(i), column_handle: if i == 0 { column } else { None }, add_row: i == last, add_column: !cut, ..TableControls::default() };
            for (kind, c) in table_controls(rect, &edges, &controls) {
                let inside = point.x >= c.origin.x && point.x <= c.origin.x + c.size.width && point.y >= c.origin.y && point.y <= c.origin.y + c.size.height;
                if inside {
                    return Some(match kind {
                        "table-row-handle" => TableControl::RowHandle(t, i),
                        "table-column-handle" => TableControl::ColumnHandle(t, column?),
                        "table-add-row" => TableControl::AddRow(t),
                        _ => TableControl::AddColumn(t),
                    });
                }
            }
        }
        None
    }

    /// A mouse press at `point`: an add bar adds at once; a handle followed by the mouse is a
    /// drag that moves its row or column, or else a click that opens its menu. False when no
    /// control is there.
    pub fn press_table(&self, view: &NSTextView, point: NSPoint) -> bool {
        let Some(control) = self.table_control_at(view, point) else { return false };
        if self.add_from(view, control) {
            return true;
        }
        let Some(window) = view.window() else { return true };
        let mut dragged = None;
        while let Some(event) = window.nextEventMatchingMask(objc2_app_kit::NSEventMask::LeftMouseDragged | objc2_app_kit::NSEventMask::LeftMouseUp) {
            let at = view.convertPoint_fromView(event.locationInWindow(), None);
            if dragged.is_some() || (at.x - point.x).hypot(at.y - point.y) > 4.0 {
                dragged = Some(at);
            }
            if event.r#type() == objc2_app_kit::NSEventType::LeftMouseUp {
                break;
            }
        }
        match dragged {
            Some(to) => self.drop_handle(view, control, to),
            None => self.show_table_menu(view, control, point),
        }
        true
    }

    /// `KNOTES_PROBE_TABLE_PRESS`: a press at `x,y` (a handle's menu printed, not shown),
    /// `x,y:n` (its item n chosen) or `x,y>x,y` (a handle dragged there).
    pub fn probe_table_press(&self, view: &NSTextView, spec: &str) {
        let point = |p: &str| p.split_once(',').and_then(|(x, y)| Some(NSPoint::new(x.parse().ok()?, y.parse().ok()?)));
        let (at, rest) = spec.split_once([':', '>']).map_or((spec, None), |(a, r)| (a, Some(r)));
        let (Some(at), Some(control)) = (point(at), point(at).and_then(|p| self.table_control_at(view, p))) else {
            println!("table-control: none");
            return;
        };
        let _ = at;
        if self.add_from(view, control) {
            return;
        }
        if spec.contains('>') {
            if let Some(to) = rest.and_then(point) {
                self.drop_handle(view, control, to);
            }
        } else if let Some(n) = rest.and_then(|r| r.parse::<usize>().ok()) {
            if let Some(&(_, action)) = self.table_menu(control).get(n) {
                self.act_on_table(view, control_table(control), action);
            }
        } else {
            let titles: Vec<&str> = self.table_menu(control).iter().map(|(title, _)| *title).collect();
            println!("table-menu: {}", titles.join(" | "));
        }
    }

    /// An add bar's row or column, added; false for a handle.
    fn add_from(&self, view: &NSTextView, control: TableControl) -> bool {
        let size = |t: usize| self.ivars().styled.borrow().tables.get(t).map(|t| (t.rows.len(), t.aligns.len()));
        match control {
            TableControl::AddRow(t) => {
                if let Some((rows, _)) = size(t) {
                    self.act_on_table(view, t, TableAction::InsertRow(rows));
                }
                true
            }
            TableControl::AddColumn(t) => {
                if let Some((_, columns)) = size(t) {
                    self.act_on_table(view, t, TableAction::InsertColumn(columns));
                }
                true
            }
            _ => false,
        }
    }

    /// A handle's menu: its titles and actions.
    fn table_menu(&self, control: TableControl) -> Vec<(&'static str, TableAction)> {
        match control {
            TableControl::RowHandle(_, row) => vec![
                (strings::TABLE_ROW_ABOVE, TableAction::InsertRow(row)),
                (strings::TABLE_ROW_BELOW, TableAction::InsertRow(row + 1)),
                (strings::TABLE_ROW_DELETE, TableAction::DeleteRow(row)),
            ],
            TableControl::ColumnHandle(_, c) => vec![
                (strings::TABLE_COLUMN_LEFT, TableAction::InsertColumn(c)),
                (strings::TABLE_COLUMN_RIGHT, TableAction::InsertColumn(c + 1)),
                (strings::TABLE_COLUMN_DELETE, TableAction::DeleteColumn(c)),
                (strings::TABLE_ALIGN_LEFT, TableAction::Align(c, Align::Left)),
                (strings::TABLE_ALIGN_CENTER, TableAction::Align(c, Align::Center)),
                (strings::TABLE_ALIGN_RIGHT, TableAction::Align(c, Align::Right)),
                (strings::TABLE_SORT_UP, TableAction::Sort(c, true)),
                (strings::TABLE_SORT_DOWN, TableAction::Sort(c, false)),
            ],
            TableControl::AddRow(_) | TableControl::AddColumn(_) => Vec::new(),
        }
    }

    /// Shows a handle's menu at `point`; the chosen item acts (`tableMenuItem:`).
    fn show_table_menu(&self, view: &NSTextView, control: TableControl, point: NSPoint) {
        let mtm = self.mtm();
        let items = self.table_menu(control);
        *self.ivars().menu_actions.borrow_mut() = items.iter().map(|(_, a)| (control_table(control), *a)).collect();
        let menu = objc2_app_kit::NSMenu::new(mtm);
        for (tag, (title, action)) in items.iter().enumerate() {
            if matches!(action, TableAction::Align(..) | TableAction::Sort(..)) && matches!(items.get(tag.wrapping_sub(1)), Some((_, TableAction::DeleteColumn(_) | TableAction::Align(_, Align::Right)))) {
                menu.addItem(&objc2_app_kit::NSMenuItem::separatorItem(mtm));
            }
            // SAFETY: the live preview is the target and lives as long as the view.
            let item = unsafe { objc2_app_kit::NSMenuItem::initWithTitle_action_keyEquivalent(objc2_app_kit::NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(sel!(tableMenuItem:)), &NSString::new()) };
            // SAFETY: as above.
            unsafe { item.setTarget(Some(self)) };
            item.setTag(tag as isize);
            menu.addItem(&item);
        }
        menu.popUpMenuPositioningItem_atLocation_inView(None, point, Some(view));
    }

    /// A handle dropped at `point`: its row or column moves to the one there.
    fn drop_handle(&self, view: &NSTextView, control: TableControl, point: NSPoint) {
        let action = {
            let Some(layout) = view.textLayoutManager() else { return };
            let Some(content) = layout.textContentManager() else { return };
            let origin = view.textContainerOrigin();
            let styled = self.ivars().styled.borrow();
            match control {
                TableControl::RowHandle(t, from) => {
                    let Some(table) = styled.tables.get(t) else { return };
                    let bands: Vec<(f64, f64)> = table.rows.iter().filter_map(|r| row_band(&layout, &content, r.range.start)).collect();
                    let y = point.y - origin.y;
                    let to = bands.iter().position(|(top, bottom)| y >= *top && y < *bottom).unwrap_or(if bands.first().is_some_and(|b| y < b.0) { 0 } else { bands.len().saturating_sub(1) });
                    TableAction::MoveRow(from, to)
                }
                TableControl::ColumnHandle(t, from) => {
                    let Some(table) = styled.tables.get(t) else { return };
                    let Some(edges) = self.ivars().table_columns.borrow().get(&table.range.start).cloned() else { return };
                    let padding = layout.textContainer().map_or(0.0, |c| c.lineFragmentPadding());
                    let x = point.x - origin.x - padding;
                    let to = edges.windows(2).position(|w| x >= w[0] && x < w[1]).unwrap_or(if x < edges[0] { 0 } else { edges.len().saturating_sub(2) });
                    TableAction::MoveColumn(from, to)
                }
                _ => return,
            }
        };
        if !matches!(action, TableAction::MoveRow(a, b) | TableAction::MoveColumn(a, b) if a == b) {
            self.act_on_table(view, control_table(control), action);
        }
    }

    /// Does `action` to table `t`, as one edit, the cursor left in the cell it concerns.
    fn act_on_table(&self, view: &NSTextView, t: usize, action: TableAction) {
        self.rewrite_table(view, t, |table| match action {
            TableAction::InsertRow(at) => {
                table.insert_row(at);
                (at.min(table.rows.len() - 1), 0)
            }
            TableAction::DeleteRow(row) => {
                table.delete_row(row);
                (row.min(table.rows.len() - 1), 0)
            }
            TableAction::InsertColumn(at) => {
                table.insert_column(at);
                (0, at.min(table.columns() - 1))
            }
            TableAction::DeleteColumn(column) => {
                table.delete_column(column);
                (0, column.min(table.columns() - 1))
            }
            TableAction::MoveRow(from, to) => {
                table.move_row(from, to);
                (to, 0)
            }
            TableAction::MoveColumn(from, to) => {
                table.move_column(from, to);
                (0, to)
            }
            TableAction::Align(column, align) => {
                table.set_align(column, align);
                (0, column)
            }
            TableAction::Sort(column, ascending) => {
                table.sort_by(column, ascending);
                (0, column)
            }
        });
    }

    /// Rewrites table `t` (its index in the note) as `change` makes it, as one edit, and puts
    /// the cursor at the end of the cell (row, column) `change` gives.
    fn rewrite_table(&self, view: &NSTextView, t: usize, change: impl FnOnce(&mut TableText) -> (usize, usize)) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let Some(range) = self.ivars().styled.borrow().tables.get(t).map(|t| t.range.clone()) else { return };
        let source = storage.string().substringWithRange(ns(&range)).to_string();
        let Some(mut table) = TableText::parse(&source) else { return };
        let (row, column) = change(&mut table);
        let mut new = table.write();
        // At the note's end the table may have no final line break.
        if !source.ends_with('\n') {
            new.pop();
        }
        self.ivars().table_editing.set(true);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&new), ns(&range)) };
        self.ivars().table_editing.set(false);
        let end = table.cell_offset(row, column) + table.rows[row][column].len();
        let at = range.start + new[..end.min(new.len())].encode_utf16().count();
        view.setSelectedRange(NSRange::new(at, 0));
    }

    /// The vault the shown notes are in: images are read from it.
    pub fn set_vault(&self, root: PathBuf) {
        *self.ivars().vault.borrow_mut() = Some(root);
        self.forget_images();
    }

    fn forget_images(&self) {
        self.ivars().images.borrow_mut().clear();
        self.ivars().bitmaps.borrow_mut().clear();
    }

    /// The size the image `run` embeds shows at: its own, or the one written after `|` (its
    /// proportions kept), no wider than the text column; read from the file's header, not
    /// decoded. With its vault path and file, and whether it is being resized. `None` when it
    /// does not load.
    fn image_size(&self, run: &ImageRun) -> Option<(String, PathBuf, NSSize, bool)> {
        let path = self.ivars().links.borrow().as_ref()?.resolve(run.link.kind, &run.link.target, &self.ivars().path.borrow())?;
        let file = self.ivars().vault.borrow().as_ref()?.join(&path);
        let natural = (*self.ivars().images.borrow_mut().entry(path.clone()).or_insert_with(|| image_loader::natural_size(&file)))?;
        if natural.width <= 0.0 || natural.height <= 0.0 {
            return None;
        }
        let ratio = natural.height / natural.width;
        let resized = self.ivars().resizing.get().filter(|r| r.0 == run.range.start).map(|r| r.1);
        let width = resized.unwrap_or(match (run.width, run.height) {
            (Some(w), Some(h)) => f64::from(w).min(f64::from(h) / ratio),
            (Some(w), None) => f64::from(w),
            (None, Some(h)) => f64::from(h) / ratio,
            (None, None) => natural.width,
        });
        // A point narrower than the column: the hidden source's tiny glyphs beside the kept
        // width would otherwise wrap onto a second line as tall as the image.
        let width = width.min(self.ivars().column.get() - 1.0).max(1.0).round();
        Some((path, file, NSSize::new(width, (width * ratio).round()), resized.is_some()))
    }

    /// The image `run` embeds, to be decoded for the size it shows at when first drawn, and
    /// that size.
    fn image(&self, run: &ImageRun) -> Option<(Rc<DeferredImage>, NSSize)> {
        let (path, file, size, resizing) = self.image_size(run)?;
        // Decoded again only to show larger; while it is being resized it is stretched.
        let mut bitmaps = self.ivars().bitmaps.borrow_mut();
        let image = match bitmaps.get(&path) {
            Some(image) if image.size.width >= size.width || resizing => Rc::clone(image),
            _ => {
                let this = objc2::rc::Weak::new(self);
                let decoded = path.clone();
                let image = Rc::new(DeferredImage::new(file, size, move || {
                    if let Some(this) = this.load() {
                        this.image_decoded(&decoded);
                    }
                }));
                bitmaps.insert(path, Rc::clone(&image));
                image
            }
        };
        Some((image, size))
    }

    /// Files changed on disk (vault paths): images among them are read again.
    pub fn files_changed(&self, view: &NSTextView, paths: &[String]) {
        let stale: Vec<&String> = {
            let images = self.ivars().images.borrow();
            paths.iter().filter(|p| images.contains_key(*p)).collect()
        };
        if stale.is_empty() {
            return;
        }
        for path in &stale {
            self.ivars().images.borrow_mut().remove(*path);
            self.ivars().bitmaps.borrow_mut().remove(*path);
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let links = self.ivars().links.borrow().clone();
        let source = self.ivars().path.borrow().clone();
        let lines: Vec<(usize, usize)> = self
            .ivars()
            .styled
            .borrow()
            .images
            .iter()
            .filter(|i| links.as_ref().and_then(|l| l.resolve(i.link.kind, &i.link.target, &source)).is_some_and(|p| stale.contains(&&p)))
            .map(|i| paragraph(&storage, ns(&i.range)))
            .collect();
        for line in lines {
            self.restyle(&storage, line);
        }
    }

    /// The note's images decoded (vault paths, sorted), and how many are kept (probe mode).
    pub fn decoded_images(&self) -> (Vec<String>, usize) {
        let bitmaps = self.ivars().bitmaps.borrow();
        let mut ready: Vec<String> = bitmaps.iter().filter(|(_, i)| i.is_decoded()).map(|(p, _)| p.clone()).collect();
        ready.sort();
        (ready, bitmaps.len())
    }

    /// Whether spelling marks stay off `range`: formulas and image embeds are no words, and
    /// on their hidden glyphs the marks showed as stray dots.
    pub fn spelling_hidden(&self, range: NSRange) -> bool {
        let styled = self.ivars().styled.borrow();
        let end = range.location + range.length;
        let meets = |r: &Range<usize>| r.start < end && r.end > range.location;
        styled.formulas.iter().any(|f| meets(&f.range)) || styled.images.iter().any(|i| meets(&i.range))
    }

    /// The pointer moved over the image on the line starting at `line`, or off images: the
    /// handle shows on the image it is over.
    pub fn hover_image(&self, view: &NSTextView, line: Option<usize>) {
        let old = self.ivars().hovered_image.replace(line);
        if old == line {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let length = storage.length();
        for at in [old, line].into_iter().flatten().filter(|&at| at < length) {
            self.restyle(&storage, paragraph(&storage, NSRange::new(at, 0)));
        }
    }

    /// The image at vault `path` is decoded: its lines are laid out again, to draw it.
    fn image_decoded(&self, path: &str) {
        let Some(view) = self.ivars().view.borrow().load() else { return };
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let links = self.ivars().links.borrow().clone();
        let source = self.ivars().path.borrow().clone();
        let lines: Vec<(usize, usize)> = self
            .ivars()
            .styled
            .borrow()
            .images
            .iter()
            .filter(|i| links.as_ref().and_then(|l| l.resolve(i.link.kind, &i.link.target, &source)).as_deref() == Some(path))
            .map(|i| paragraph(&storage, ns(&i.range)))
            .collect();
        for line in lines {
            self.restyle(&storage, line);
        }
    }

    /// The note scrolled: images more than a screen above or below the view's visible part
    /// are let go (decoded again when they come back), checked every quarter screen.
    fn let_far_images_go(&self, view: &NSTextView) {
        let visible = view.visibleRect();
        let top = visible.origin.y - view.textContainerOrigin().y;
        let screen = visible.size.height;
        if self.ivars().images_checked_at.get().is_some_and(|at| (at - top).abs() < screen / 4.0) {
            return;
        }
        self.ivars().images_checked_at.set(Some(top));
        for image in self.ivars().bitmaps.borrow().values() {
            let Some(at) = image.drawn_at.get() else { continue };
            if at + image.size.height < top - screen || at > top + 2.0 * screen {
                image.let_go();
            }
        }
    }

    /// An image away from the cursor: its line's source hidden, the line centred and as tall
    /// as the image, the image's width kept by kerning (ADR 0020). With the cursor on its line:
    /// the source, and room below it for the image.
    fn paint_image<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, run: &ImageRun, revealed: bool) {
        let Some((_, _, size, _)) = self.image_size(run) else { return };
        let line = paragraph(storage, ns(&run.range));
        let reserved = size.height + 2.0 * PREVIEW_GAP;
        if revealed {
            let para = palette.para(Para { spacing_after: reserved, ..Para::default() });
            canvas.paint(line.0..line.1, |p| p.para = para);
            return;
        }
        let look = &self.ivars().look;
        let text = storage.string();
        let end = if line.1 > line.0 && matches!(text.characterAtIndex(line.1 - 1), 0x0a | 0x0d) { line.1 - 1 } else { line.1 };
        let hidden = palette.look(&look.hidden, &look.clear, "image");
        let kern = palette.kern(size.width);
        canvas.paint(line.0..end, |p| {
            p.look = hidden;
            p.kern = 0;
        });
        canvas.paint(line.0..line.0 + 1, |p| p.kern = kern);
        let para = palette.para(Para { centred: true, min_line: reserved, max_line: reserved, ..Para::default() });
        canvas.paint(line.0..line.1, |p| p.para = para);
    }

    /// The image on the line starting at `line` dragged to `width` points: shown so at once;
    /// when `done`, the width is written into its embed (`|300`), one edit to undo, the cursor
    /// staying where it was.
    pub fn resize_image(&self, view: &NSTextView, line: usize, width: f64, done: bool) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let run = self.ivars().styled.borrow().images.iter().find(|i| paragraph(&storage, ns(&i.range)).0 == line).cloned();
        let Some(run) = run else { return };
        let width = width.round().clamp(MIN_IMAGE_WIDTH, (self.ivars().column.get() - 1.0).max(MIN_IMAGE_WIDTH));
        self.ivars().resizing.set(Some((run.range.start, width)));
        if !done {
            self.restyle(&storage, paragraph(&storage, ns(&run.range)));
            return;
        }
        self.ivars().resizing.set(None);
        let source = storage.string().substringWithRange(ns(&run.range)).to_string();
        let new = set_image_width(&source, width as u32);
        let selection = view.selectedRange();
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&new), ns(&run.range)) };
        let grown = new.encode_utf16().count() as isize - run.range.len() as isize;
        let keep = |at: usize| if at >= run.range.end { (at as isize + grown).max(0) as usize } else { at };
        view.setSelectedRange(NSRange::new(keep(selection.location), selection.length));
    }

    /// The file of the image on the line starting at `line`.
    pub fn image_file(&self, view: &NSTextView, line: usize) -> Option<PathBuf> {
        let storage = unsafe { view.textStorage() }?;
        let styled = self.ivars().styled.borrow();
        let run = styled.images.iter().find(|i| paragraph(&storage, ns(&i.range)).0 == line)?;
        let path = self.ivars().links.borrow().as_ref()?.resolve(run.link.kind, &run.link.target, &self.ivars().path.borrow())?;
        Some(self.ivars().vault.borrow().as_ref()?.join(path))
    }

    /// The text column changed width: images wider than it, or that were, fit again.
    fn fit_images(&self, view: &NSTextView) {
        let column = text_column(view);
        if self.ivars().column.replace(column) == column {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let paragraphs: Vec<(usize, usize)> = self.ivars().styled.borrow().images.iter().map(|i| paragraph(&storage, ns(&i.range))).collect();
        for para in paragraphs {
            self.restyle(&storage, para);
        }
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
        let (Some(range), false) = (range, styled.formulas.is_empty() && styled.runs.is_empty() && styled.code_blocks.is_empty() && styled.tables.is_empty()) else {
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
        for run in styled.images.iter().filter(|i| i.range.start < to && i.range.end > from) {
            let Some((image, size)) = self.image(run) else { continue };
            let (anchor, at) = if overlaps(&run.range, revealed) {
                (Anchor::Below, run.range.end.saturating_sub(1))
            } else {
                (Anchor::Line, paragraph(&storage, ns(&run.range)).0)
            };
            if (from..to).contains(&at) {
                let handle = anchor == Anchor::Line && self.ivars().hovered_image.get() == Some(paragraph(&storage, ns(&run.range)).0);
                marks.push(Mark::Image { offset: at - from, image, size, anchor, handle });
            }
        }
        let centre_x = self.ivars().look.body.xHeight() / 2.0;
        let text = storage.string();
        for end in external_link_ends(&styled, (from, to), self.ivars().revealed.get()) {
            // At the hidden `]`, whose own room follows it; or where a bare address ends, whose
            // location sits halfway into the room kerned after the address's last character
            // (where a caret would go) — unless the line ends there and the kerning is dropped.
            let (offset, shift) = if end.bare {
                let line_end = end.kern_at + 1 >= text.length() || matches!(text.characterAtIndex(end.kern_at + 1), 0x0a | 0x0d);
                (end.kern_at + 1, if line_end { 3.0 } else { 3.0 - EXTERNAL_ROOM / 2.0 })
            } else {
                (end.kern_at, 3.0)
            };
            if offset >= from && offset <= to {
                marks.push(Mark::External { offset: offset - from, centre: centre_x + 1.5, shift });
            }
        }
        let revealed = self.ivars().revealed.get();
        let look = &self.ivars().look;
        for callout in styled.callouts.iter().filter(|c| c.range.start < to && c.range.end > from) {
            let (colour, symbol) = callout_look(&callout.kind);
            let colour = look.callouts[colour].clone();
            marks.push(Mark::CalloutBox { colour: colour.clone(), top: from <= callout.range.start, bottom: to >= callout.range.end });
            if callout.marker.start >= from && callout.marker.start < to && !overlaps(&callout.marker, revealed) {
                let label = callout.title.is_empty().then(|| callout_name(&callout.kind));
                marks.push(Mark::CalloutIcon { offset: callout.marker.start - from, centre: look.body.capHeight() / 2.0, colour, symbol, label, font: look.bold.clone() });
            }
        }
        for (t, table) in styled.tables.iter().enumerate().filter(|(_, t)| t.range.start < to && t.range.end > from) {
            let Some(columns) = self.ivars().table_columns.borrow().get(&table.range.start).cloned() else { continue };
            let scrolled = self.ivars().table_scroll.borrow().get(&t).is_some_and(|&first| first > 0);
            let hover = self.ivars().hovered_table.get().filter(|h| h.0 == table.range.start);
            let last = table.rows.len().saturating_sub(1);
            for (i, _) in table.rows.iter().enumerate().filter(|(_, r)| (from..to).contains(&r.range.start)) {
                let controls = hover.map_or(TableControls::default(), |(_, row, column)| TableControls {
                    handle: row == Some(i),
                    column_handle: if i == 0 { column } else { None },
                    add_row: i == last,
                    add_column: true,
                    add_column_plus: i == last / 2,
                    first: i == 0,
                    last: i == last,
                });
                marks.push(Mark::TableRow { columns: Rc::clone(&columns), header: i == 0, top: i == 0, scrolled, controls });
            }
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
                // A callout's lines show its box instead of bars.
                if !styled.callouts.iter().any(|c| run.range.start >= c.range.start && run.range.start < c.range.end) {
                    marks.extend((0..depth).map(|level| Mark::QuoteBar { x: f64::from(level) * QUOTE_INDENT }));
                }
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
        // An image may have come, gone or changed.
        self.forget_images();
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
        let Some(link) = self.followed_link(index, cmd) else { return false };
        if let Some(callback) = self.ivars().on_link.borrow().as_ref() {
            callback(&link);
        }
        true
    }

    /// Whether a plain click on character `index` acts (follows a link, ticks a task): the
    /// pointer shows a hand there.
    pub fn clickable(&self, index: usize) -> bool {
        self.followed_link(index, false).is_some() || self.task_at(index).is_some()
    }

    /// The link a click on character `index` follows: where its markup is hidden, or anywhere
    /// with Cmd.
    fn followed_link(&self, index: usize, cmd: bool) -> Option<Link> {
        let styled = self.ivars().styled.borrow();
        let revealed = self.ivars().revealed.get();
        let in_cursor_line = index >= revealed.0 && index < revealed.1;
        // An image is no link to follow: a click puts the cursor in its source.
        let image = |l: &&LinkRun| styled.images.iter().any(|i| i.range == l.range);
        link_at(&styled, index).filter(|l| (cmd || !in_cursor_line) && !image(l)).map(|link| link.link.clone())
    }

    /// The task marker whose drawn circle holds character `index` (not one showing its source).
    fn task_at(&self, index: usize) -> Option<Run> {
        let styled = self.ivars().styled.borrow();
        let after = styled.runs.partition_point(|r| r.range.start <= index);
        styled.runs[after.saturating_sub(MARKER_REACH)..after]
            .iter()
            .rev()
            .find(|r| matches!(r.style, Style::ListMarker { kind: ListKind::Task { .. }, .. }) && r.range.contains(&index) && !touches(&r.range, self.ivars().selection.get()))
            .cloned()
    }

    /// Ticks or unticks the task whose drawn box holds character `index`: an edit of the
    /// text, so it can be undone and is saved; the cursor stays.
    fn toggle_task_at(&self, view: &NSTextView, index: usize) -> bool {
        let at = {
            let Some(task) = self.task_at(index) else { return false };
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
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        Self::describe_with(view, unsafe { NSStrikethroughStyleAttributeName }, |v| v.downcast_ref::<NSNumber>().is_some_and(|n| n.integerValue() != 0))
    }

    /// The pieces of text with a background (inline code), `|` between those apart.
    pub fn describe_shaded(view: &NSTextView) -> String {
        // SAFETY: AppKit's attribute name constant lives for the whole process.
        Self::describe_with(view, unsafe { NSBackgroundColorAttributeName }, |_| true)
    }

    /// The pieces of text whose `key` attribute `has` holds for, `|` between those apart.
    fn describe_with(view: &NSTextView, key: &NSString, has: impl Fn(&AnyObject) -> bool) -> String {
        let Some(storage) = (unsafe { view.textStorage() }) else { return String::new() };
        let utf16: Vec<u16> = storage.string().to_string().encode_utf16().collect();
        let (mut pieces, mut at, mut last_end) = (String::new(), 0, None);
        while at < storage.length() {
            let mut range = NSRange::new(0, 0);
            // SAFETY: a valid index and an out pointer for the range.
            let value = unsafe { storage.attribute_atIndex_effectiveRange(key, at, &mut range) };
            let end = (range.location + range.length).max(at + 1);
            if value.is_some_and(|v| has(&v)) {
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

/// Width of the text column: the text container less its line padding.
fn text_column(view: &NSTextView) -> f64 {
    unsafe { view.textContainer() }.map_or(READABLE_WIDTH, |c| c.size().width - 2.0 * c.lineFragmentPadding())
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

/// The table a control belongs to.
fn control_table(control: TableControl) -> usize {
    match control {
        TableControl::RowHandle(t, _) | TableControl::ColumnHandle(t, _) | TableControl::AddRow(t) | TableControl::AddColumn(t) => t,
    }
}

/// The top and bottom of the row starting at `offset` (text container coordinates), without
/// the empty line after the note's final line break.
fn row_band(layout: &NSTextLayoutManager, content: &objc2_app_kit::NSTextContentManager, offset: usize) -> Option<(f64, f64)> {
    // A restyle (the pointer coming over the table restyles it) leaves the rows to lay out.
    layout.textViewportLayoutController().layoutViewport();
    let location = content.locationFromLocation_withOffset(&content.documentRange().location(), offset as isize)?;
    let fragment = layout.textLayoutFragmentForLocation(&location)?;
    let top = fragment.layoutFragmentFrame().origin.y;
    Some((top, top + crate::preview_fragment::row_height(&fragment)))
}

/// The table, row and cell whose text holds `index` (its end too).
fn cell_at(styled: &Highlight, index: usize) -> Option<(usize, usize, usize)> {
    styled.tables.iter().enumerate().filter(|(_, t)| t.range.start <= index && index <= t.range.end).find_map(|(t, table)| {
        table.rows.iter().enumerate().find_map(|(r, row)| row.cells.iter().position(|c| c.start <= index && index <= c.end).map(|c| (t, r, c)))
    })
}

/// The nearest cell edge to `index` when it lies on a table's line outside its cells (on a
/// hidden pipe or space, or the folded delimiter row).
fn nearest_cell_edge(styled: &Highlight, index: usize) -> Option<usize> {
    let table = styled.tables.iter().find(|t| t.range.start <= index && index < t.range.end)?;
    if table.delimiter.start <= index && index <= table.delimiter.end {
        return table.rows.first()?.cells.last().map(|c| c.end);
    }
    let row = table.rows.iter().find(|r| r.range.start <= index && index <= r.range.end)?;
    row.cells.iter().flat_map(|c| [c.start, c.end]).min_by_key(|p| p.abs_diff(index))
}

/// `text` fit for a table cell: a `|` escaped, line breaks as spaces.
fn cell_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut escaped = false;
    for c in text.chars() {
        match c {
            '|' if !escaped => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
        escaped = c == '\\' && !escaped;
    }
    out
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

/// Where the room for a website link's arrow is kept: the first character of its hidden
/// `](url)`, or the last of a bare address (`bare`).
struct ArrowRoom {
    kern_at: usize,
    bare: bool,
}

/// The arrow rooms of the links to websites meeting `region`, outside the `revealed`
/// paragraph (whose source shows).
fn external_link_ends(styled: &Highlight, region: (usize, usize), revealed: (usize, usize)) -> Vec<ArrowRoom> {
    let first = styled.links.partition_point(|l| l.range.end <= region.0);
    styled.links[first..]
        .iter()
        .take_while(|l| l.range.start < region.1)
        .filter(|l| l.link.kind == LinkKind::External && !overlaps(&l.range, revealed))
        .filter_map(|l| {
            let after = styled.markup.partition_point(|m| m.start <= l.range.start);
            let close = styled.markup[after..].iter().take_while(|m| m.start < l.range.end).find(|m| m.end == l.range.end);
            let opened = styled.markup[..after].last().is_some_and(|m| m.start == l.range.start);
            match close {
                Some(m) => Some(ArrowRoom { kern_at: m.start, bare: false }),
                // No markup at either end: a bare address.
                None if !opened => Some(ArrowRoom { kern_at: l.range.end - 1, bare: true }),
                None => None,
            }
        })
        .filter(|room| room.kern_at >= region.0 && room.kern_at < region.1)
        .collect()
}

/// Room a hidden callout marker keeps for its icon.
const CALLOUT_ICON_ROOM: f64 = 20.0;

/// A callout kind's colour (index into the look's callout colours) and icon, as Obsidian's.
fn callout_look(kind: &str) -> (usize, &'static str) {
    match kind {
        "abstract" | "summary" | "tldr" => (1, "list.bullet.clipboard"),
        "info" => (0, "info.circle"),
        "todo" => (0, "checkmark.circle"),
        "tip" | "hint" | "important" => (1, "flame"),
        "success" | "check" | "done" => (2, "checkmark"),
        "question" | "help" | "faq" => (3, "questionmark.circle"),
        "warning" | "caution" | "attention" => (3, "exclamationmark.triangle"),
        "failure" | "fail" | "missing" => (4, "xmark"),
        "danger" | "error" => (4, "bolt"),
        "bug" => (4, "ladybug"),
        "example" => (5, "list.bullet"),
        "quote" | "cite" => (6, "quote.opening"),
        _ => (0, "pencil"),
    }
}

/// A callout's name when no title is written: its kind, capitalised (as Obsidian shows it).
fn callout_name(kind: &str) -> String {
    let mut chars = kind.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
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
