//! Live preview on TextKit 2 (spike, ADR 0011): styles from `knotes_editor::highlight`, and the
//! markup (`**`, `## `, `[[Note|`) hidden everywhere except the paragraph with the cursor.
//! Every styled range also carries a `knotes.style` attribute, so tests can read what is shown.
//! A module per part: restyling, tables, images, formulas, code blocks, the marks drawn in place
//! of hidden markup, embedded notes, links, task boxes, found words and the probe's words.

mod code;
mod describe;
mod embeds;
mod find;
mod formulas;
mod images;
mod links;
mod marks;
mod restyle;
mod tables;
mod tasks;

use self::{code::*, formulas::*, links::*, restyle::*};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dispatch2::DispatchQueue;

use knotes_core::{Link, LinkKind, LinkResolver};
use knotes_editor::{TableTyping, cell_at, nearest_cell_edge, CalloutRun, Canvas, Edit, FormulaCache, FormulaImage, FormulaRun, FormulaStyle, Highlight, ImageRun, LinkRun, ListKind, Run, Style, TableRun, TableText, Align, changed_region, highlight, set_image_width, CodeHighlighter, CodeRun, CodeToken, utf16_offsets};

use crate::image_loader::{self, DeferredImage};
use crate::preview_fragment::{Anchor, BULLET_ROOM, EXTERNAL_ROOM, Mark, PDF_CAPTION, PREVIEW_GAP, PreviewFragment, TASK_ROOM, TableControls, table_controls};
use crate::strings;
use crate::text_paint::{MARKED, Paint, PaintCache, Palette, Para, RAISED, SHADED, STRUCK};
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
    NSUndoManager, NSValue,
};

pub const STYLE_KEY: &str = "knotes.style";
/// Longest line of note text, as Obsidian's "readable line length" (700 CSS px = 700 pt).
pub const READABLE_WIDTH: f64 = 700.0;
/// Side margin when the column is narrower than the readable width.
const MIN_MARGIN: f64 = 32.0;
const TOP_MARGIN: f64 = 24.0;
const BODY_SIZE: f64 = 15.0;
/// A footnote reference's label: its size and how far above the baseline, of the body's.
const FOOTNOTE_SCALE: f64 = 0.7;
const FOOTNOTE_RAISE: f64 = 0.4;
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
/// How long after a table's last sideways scroll step its rows out of view are set.
const CATCH_UP_MS: u64 = 150;
/// Widths of pieces of table cells kept at most.
const MAX_PIECE_WIDTHS: usize = 20_000;

struct Look {
    body: Retained<NSFont>,
    bold: Retained<NSFont>,
    italic: Retained<NSFont>,
    mono: Retained<NSFont>,
    /// Keywords in highlighted code, as Xcode sets them.
    mono_bold: Retained<NSFont>,
    /// A digit's width in the code font, once measured (code line numbers).
    mono_digit: Cell<Option<f64>>,
    /// Highlighted code's colours (ADR 0025), by `CodeToken`'s `code_look` index: Xcode's,
    /// light and dark.
    code_colors: Vec<Retained<NSColor>>,
    /// List numbers: digits of one width, so items' text lines up.
    digits: Retained<NSFont>,
    headings: Vec<Retained<NSFont>>,
    /// A footnote reference's raised label.
    footnote: Retained<NSFont>,
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
    /// A card's bar (ADR 0037): lilac, the owner's choice; and its deck's mark, the same hue
    /// brighter (the owner found the bar's too pale for a name).
    card: Retained<NSColor>,
    card_deck: Retained<NSColor>,
    clear: Retained<NSColor>,
    /// Attribute sets and paragraph styles, made once and shared.
    paint: PaintCache,
    /// Widths of a digit, `.`, `)` and a space in the digits font, once measured.
    number_widths: Cell<Option<[f64; 4]>>,
    /// Widths of characters in the body font, once measured.
    body_widths: RefCell<std::collections::HashMap<u16, f64>>,
    /// Widths of pieces of table cells by text and font, once measured.
    piece_widths: RefCell<std::collections::HashMap<(String, usize), f64>>,
    /// Where the characters of pieces of table cells start, by text and font, once measured.
    char_starts: RefCell<std::collections::HashMap<(String, usize), CharStarts>>,
}


type OnChange = Box<dyn Fn()>;
/// Where each character of a piece of text starts: (UTF-16 units in, points in).
type CharStarts = Rc<Vec<(usize, f64)>>;
/// A table's column widths, sideways scroll (its bits) and start, as laid out.
type TableLayout = (Vec<f64>, u64, usize);
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

/// A right click's table actions: the table's index, the items, and whether they stand alone.
type ContextItems = (usize, Vec<(&'static str, TableAction)>, bool);

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
    /// Called to open a link's document in a new tab (its context menu, ADR 0024), and the
    /// link the menu was opened on.
    on_link_new_tab: RefCell<Option<OnLink>>,
    menu_link: RefCell<Option<Link>>,
    /// Called after the cursor moved.
    on_cursor: RefCell<Option<OnChange>>,
    /// Called after the note scrolled.
    on_scroll: RefCell<Option<OnChange>>,
    /// Offered key commands (`moveDown:`, `insertNewline:`…) first; true when it took one.
    on_command: RefCell<Option<OnCommand>>,
    /// Resolves the note's links, to dim those to missing notes.
    links: RefCell<Option<Arc<dyn LinkResolver>>>,
    /// The ids of the shown note's paused cards (ADR 0037): those show muted.
    paused_cards: RefCell<std::collections::HashSet<String>>,
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
    /// Page counts of PDFs by vault path, once read; `None` for one that does not open.
    pdf_pages: RefCell<HashMap<String, Option<usize>>>,
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
    /// Styling a note embedded in another, in a view of its own (ADR 0034): no centred column.
    embedded: Cell<bool>,
    /// A note only shown, never typed in (the vault's search): no paragraph's markup shows for
    /// the cursor's sake.
    shown_only: Cell<bool>,
    /// Each embedded note's box height (by `embed_key`): the room its line keeps.
    embed_heights: RefCell<HashMap<String, f64>>,
    /// An embedded note's own undo steps (the window's would undo the note around it).
    undo: RefCell<Option<Retained<NSUndoManager>>>,
    /// The table the pointer is over (where it starts), and the row and column under it.
    hovered_table: Cell<Option<TableHover>>,
    /// The code block under the pointer (its start): its copy button shows.
    hovered_code: Cell<Option<usize>>,
    /// Where a code block's copy button copies to: the general pasteboard, or the probe's.
    copy_board: RefCell<Option<Retained<objc2_app_kit::NSPasteboard>>>,
    /// How far each wide table is scrolled sideways, in points (by its index in the note).
    table_scroll: RefCell<HashMap<usize, f64>>,
    /// Each table's column widths with their padding (by its index), as last laid out.
    table_widths: RefCell<HashMap<usize, Vec<f64>>>,
    /// While a table scrolls sideways: the part of the text in view, the only one set again.
    apply_within: Cell<Option<(usize, usize)>>,
    /// A table whose rows out of view still wait for its last sideways scroll, and the
    /// generation of the catch-up scheduled for it.
    stale_table: Cell<Option<usize>>,
    catch_up: Cell<u64>,
    /// Each table's column widths, sideways scroll (its bits) and start, as last laid out (by its
    /// index), and whether a restyle changed any: then all of its rows are set again.
    table_layouts: RefCell<HashMap<usize, TableLayout>>,
    /// Code blocks' tokens (UTF-16, from the code's start), by the hash of their language and
    /// code: highlighted again only when they change.
    code_tokens: RefCell<HashMap<u64, CodeTokens>>,
    /// Each code block's highlighter (by its place among the note's blocks), kept between
    /// edits so typing parses again only the lines it changes.
    code_highlighters: RefCell<HashMap<usize, CodeHighlighter>>,
    /// Where each code line of a block starts (UTF-16), by the block's range.
    code_lines: RefCell<HashMap<(usize, usize), CodeLines>>,
    tables_changed: Cell<bool>,
    /// The actions of the table menu being shown, by item tag, with the table's index.
    menu_actions: RefCell<Vec<(usize, TableAction)>>,
    /// A table edit of the live preview's own is under way: not checked against the table's
    /// structure.
    table_editing: Cell<bool>,
    /// Text typed at the start of a table (a note starting with one): where its line ends, for
    /// `textDidChange` to break it there, so the table keeps its line.
    line_above_table: Cell<Option<usize>>,
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
                self.break_line_above_table(&view);
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
        /// "Відкрити в новій вкладці" in a link's context menu.
        #[unsafe(method(openLinkInNewTab:))]
        fn open_link_in_new_tab(&self, _sender: Option<&AnyObject>) {
            self.open_menu_link();
        }

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
                // Rows coming into view must not show a sideways scroll they missed.
                self.catch_up_table(&view);
            }
            if let Some(callback) = self.ivars().on_scroll.borrow().as_ref() {
                callback();
            }
        }

        #[unsafe(method(frameDidChange:))]
        fn frame_did_change(&self, notification: &NSNotification) {
            if let Some(view) = notification.object().and_then(|o| o.downcast::<NSTextView>().ok()) {
                if !self.ivars().embedded.get() {
                    fit_readable_width(&view);
                }
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
        /// An embedded note undoes its own edits; the note's view the window's (as without this).
        #[unsafe(method_id(undoManagerForTextView:))]
        fn undo_manager_for(&self, view: &NSTextView) -> Option<Retained<NSUndoManager>> {
            self.ivars().undo.borrow().clone().or_else(|| view.window().and_then(|w| w.undoManager()))
        }

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
            mono_bold: NSFont::monospacedSystemFontOfSize_weight(BODY_SIZE - 1.0, 0.4),
            mono_digit: Cell::new(None),
            code_colors: CODE_COLORS.iter().map(|&(light, dark)| appearance_color(light, dark)).collect(),
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
            card: appearance_color(0x9A7F_D9, 0xB7A3_F5),
            card_deck: appearance_color(0x7A4D_F0, 0xA78B_FF),
            clear: NSColor::clearColor(),
            paint: PaintCache::new(STYLE_KEY, BODY_SIZE * FOOTNOTE_RAISE),
            footnote: NSFont::systemFontOfSize(BODY_SIZE * FOOTNOTE_SCALE),
            number_widths: Cell::new(None),
            body_widths: RefCell::new(std::collections::HashMap::new()),
            piece_widths: RefCell::new(std::collections::HashMap::new()),
            char_starts: RefCell::new(std::collections::HashMap::new()),
            body,
        };
        let ivars = Ivars {
            on_change: RefCell::new(None),
            on_link: RefCell::new(None),
            on_link_new_tab: RefCell::new(None),
            menu_link: RefCell::new(None),
            on_cursor: RefCell::new(None),
            on_scroll: RefCell::new(None),
            on_command: RefCell::new(None),
            links: RefCell::new(None),
            paused_cards: RefCell::default(),
            path: RefCell::new(String::new()),
            formula_cache: RefCell::new(FormulaCache::default()),
            dark: Cell::new(false),
            vault: RefCell::new(None),
            images: RefCell::new(HashMap::new()),
            bitmaps: RefCell::new(HashMap::new()),
            pdf_pages: RefCell::new(HashMap::new()),
            column: Cell::new(READABLE_WIDTH),
            resizing: Cell::new(None),
            table_columns: RefCell::new(HashMap::new()),
            hovered_image: Cell::new(None),
            embedded: Cell::new(false),
            shown_only: Cell::new(false),
            embed_heights: RefCell::new(HashMap::new()),
            undo: RefCell::new(None),
            table_editing: Cell::new(false),
            line_above_table: Cell::new(None),
            menu_actions: RefCell::new(Vec::new()),
            table_scroll: RefCell::new(HashMap::new()),
            table_widths: RefCell::new(HashMap::new()),
            table_layouts: RefCell::new(HashMap::new()),
            code_tokens: RefCell::new(HashMap::new()),
            code_highlighters: RefCell::new(HashMap::new()),
            code_lines: RefCell::new(HashMap::new()),
            apply_within: Cell::new(None),
            stale_table: Cell::new(None),
            catch_up: Cell::new(0),
            tables_changed: Cell::new(false),
            hovered_table: Cell::new(None),
            hovered_code: Cell::new(None),
            copy_board: RefCell::new(None),
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

    pub fn on_scroll(&self, callback: impl Fn() + 'static) {
        *self.ivars().on_scroll.borrow_mut() = Some(Box::new(callback));
    }

    /// Makes `view` keep its text at the readable width as it resizes, and draw formulas.
    /// The note is only shown, never typed in: no paragraph shows its markup for the cursor.
    pub fn show_only(&self) {
        self.ivars().shown_only.set(true);
    }

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

    /// The vault path of the note shown.
    pub fn path(&self) -> String {
        self.ivars().path.borrow().clone()
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

    /// Shows `source` in `view` with live preview styles.
    pub fn show(&self, view: &NSTextView, path: &str, source: &str) {
        self.clear_found(view);
        self.ivars().hovered_image.set(None);
        // The last note's images go with it, and its tables' scrolling.
        self.forget_images();
        self.ivars().table_scroll.borrow_mut().clear();
        self.ivars().table_layouts.borrow_mut().clear();
        self.ivars().code_tokens.borrow_mut().clear();
        self.ivars().code_highlighters.borrow_mut().clear();
        self.ivars().code_lines.borrow_mut().clear();
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
        let para = self.cursor_paragraph(&storage, view);
        self.ivars().revealed.set(para);
        self.ivars().selection.set(span(view.selectedRange()));
        self.ivars().dark.set(is_dark(view));
        self.restyle(&storage, (0, storage.length()));
    }

    /// A mouse press at `point`: a table's control, or a PDF's preview (opened in place of the
    /// note, ADR 0022). False when neither is there.
    pub fn press(&self, view: &NSTextView, point: NSPoint) -> bool {
        self.press_code_copy(view, point) || self.press_table(view, point) || self.open_pdf_at(view, point)
    }

    /// Whether spelling marks stay off `range`: formulas and image embeds are no words, and
    /// on their hidden glyphs the marks showed as stray dots.
    pub fn spelling_hidden(&self, range: NSRange) -> bool {
        let styled = self.ivars().styled.borrow();
        let end = range.location + range.length;
        let meets = |r: &Range<usize>| r.start < end && r.end > range.location;
        styled.formulas.iter().any(|f| meets(&f.range)) || styled.images.iter().any(|i| meets(&i.range))
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

    /// The shown note's paused cards, by id: they show muted. Restyles when they changed.
    pub fn set_paused_cards(&self, view: &NSTextView, ids: std::collections::HashSet<String>) {
        if *self.ivars().paused_cards.borrow() == ids {
            return;
        }
        *self.ivars().paused_cards.borrow_mut() = ids;
        if let Some(storage) = unsafe { view.textStorage() } {
            self.restyle(&storage, (0, storage.length()));
        }
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

}

/// A colour that is `light` (0xRRGGBB, sRGB) in the light appearance and `dark` in the dark
/// one, following the view's appearance as system colours do.
pub fn appearance_color(light: u32, dark: u32) -> Retained<NSColor> {
    let srgb = |rgb: u32| NSColor::colorWithSRGBRed_green_blue_alpha(f64::from((rgb >> 16) & 0xFF) / 255.0, f64::from((rgb >> 8) & 0xFF) / 255.0, f64::from(rgb & 0xFF) / 255.0, 1.0);
    let (light, dark) = (srgb(light), srgb(dark));
    let provider = block2::RcBlock::new(move |appearance: std::ptr::NonNull<objc2_app_kit::NSAppearance>| -> std::ptr::NonNull<NSColor> {
        // SAFETY: AppKit passes the appearance a colour is asked for.
        let appearance = unsafe { appearance.as_ref() };
        // SAFETY: AppKit's appearance name constants live for the whole process.
        let (aqua, dark_aqua) = unsafe { (objc2_app_kit::NSAppearanceNameAqua, objc2_app_kit::NSAppearanceNameDarkAqua) };
        let names = objc2_foundation::NSArray::from_slice(&[aqua, dark_aqua]);
        let is_dark = appearance.bestMatchFromAppearancesWithNames(&names).is_some_and(|n| n.isEqualToString(dark_aqua));
        // The colours live as long as the block, which the colour keeps.
        std::ptr::NonNull::from(if is_dark { &*dark } else { &*light })
    });
    // SAFETY: the block returns one of two colours it keeps alive.
    unsafe { NSColor::colorWithName_dynamicProvider(None, &provider) }
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

/// The text in the view's viewport (what TextKit lays out around what shows), as UTF-16
/// offsets.
fn viewport_range(view: &NSTextView) -> Option<(usize, usize)> {
    let layout = view.textLayoutManager()?;
    let content = layout.textContentManager()?;
    let range = layout.textViewportLayoutController().viewportRange()?;
    let start = content.documentRange().location();
    let from = content.offsetFromLocation_toLocation(&start, &range.location()).max(0) as usize;
    let to = content.offsetFromLocation_toLocation(&start, &range.endLocation()).max(0) as usize;
    Some((from, to))
}

/// Width of `text` laid out on one line in `font`.
fn text_width(text: &NSString, font: &NSFont) -> f64 {
    // SAFETY: AppKit's attribute name constant lives for the whole process.
    let attrs = NSDictionary::from_slices(&[unsafe { NSFontAttributeName }], &[font.as_ref() as &AnyObject]);
    // SAFETY: a font is the font attribute's value type.
    unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), text, Some(&attrs)) }.size().width
}

fn is_dark(view: &NSTextView) -> bool {
    objc2_app_kit::NSAppearanceCustomization::effectiveAppearance(view).name().to_string().contains("Dark")
}

fn paragraph(storage: &NSTextStorage, selection: NSRange) -> (usize, usize) {
    let p = storage.string().paragraphRangeForRange(selection);
    (p.location, p.location + p.length)
}

fn span(selection: NSRange) -> (usize, usize) {
    (selection.location, selection.location + selection.length)
}

/// Where the first line of `fragment` ends, from its top; none before it is laid out (AppKit
/// gives no line fragments then, though declared to).
pub fn first_line_bottom(fragment: &NSTextLayoutFragment) -> Option<f64> {
    // SAFETY: `textLineFragments` takes no argument and gives an array, or nil before layout.
    let lines: Option<Retained<NSArray<objc2_app_kit::NSTextLineFragment>>> = unsafe { msg_send![fragment, textLineFragments] };
    let bounds = lines?.firstObject()?.typographicBounds();
    Some(bounds.origin.y + bounds.size.height)
}

/// A paragraph no range meets: nothing revealed.
const NOWHERE: (usize, usize) = (usize::MAX, usize::MAX);

fn overlaps(m: &Range<usize>, para: (usize, usize)) -> bool {
    m.start < para.1 && m.end > para.0 || (m.start == para.1 && para.0 == para.1)
}

fn ns(r: &Range<usize>) -> NSRange {
    NSRange::new(r.start, r.end - r.start)
}
