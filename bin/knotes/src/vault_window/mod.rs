//! A window showing one vault: owns the AppKit objects and applies session events.

mod documents;
mod editing;
mod tabs;
mod panel;
mod list;
mod search;
mod find;
mod completion;
mod links;
mod embeds;
mod trash;
mod spaces;
mod probe_actions;
mod ai;
mod cards;

use self::{completion::*, editing::*, find::*, list::*};

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};

use knotes_app::{Moves, Relinks, embed_images, list_scope, pasted_image_name, plan_relinks, 
    FolderNode, FolderRules, History, ListFilter, ListOption, ListSettings, Outline, OutlineEntry, PanelMode, SearchMode, Sidebar, SidebarItem, SidebarMode, Tabs, AfterClose, TabPlace, TagNode, parse_search,
    search_list, with_search_mode, build_note_list, new_notes_folder, ListItem, trash_days, SpaceFolders, SpaceList, SpaceTags, RestPart, suggest_tags, folder_tree_with, unique_folder_path, space_notes_folder, list_space_scope,
    note_file_stem, resolve_list_settings, unique_file_path, unique_note_path,
};
use knotes_core::{AttachmentStore, CatalogEntry, FileKind, FolderRulesStore, Link, LinkKind, LinkResolver, NoteMover, NoteParser, NoteRow, NoteWriter, TagIndex, TrashEntry, TrashStore, VaultReader};
use knotes_md::MarkdownParser;
use knotes_core::TitleSearch;
use knotes_editor::{DiskChange, Document, LinkCompletion, TagCompletion, embedded_part, conflict_path, link_completion, note_headings, subpath_offset, tag_completion, tag_typed, utf16_offsets};
use knotes_index::{LinkTargets, SqliteIndex};
use knotes_search::TitleIndex;
use knotes_session::{Event, Snapshot};
use knotes_spaces::{FolderSpace, Space, SpaceColor, Spaces, new_space_id, set_folder_space, set_spaces};
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::runtime::{ProtocolObject, Sel};
use objc2::sel;
use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF, NSModalResponse, NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSTextField, NSTextInputClient, NSTextView, NSView, NSViewController, NSWindow, NSWorkspace};
use crate::pdf_viewer::PdfViewer;
use objc2_foundation::{NSDate, NSDateFormatter, NSDictionary, NSIndexSet, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSPoint, NSRange, NSString, NSURL};
use objc2_app_kit::NSTextElementProvider;

use crate::chat_panel::ChatPanel;
use crate::collapse_observer::CollapseObserver;
use crate::connections_panel::{Connections, ConnectionsPanel};
use crate::outline_panel::OutlinePanel;
use crate::completion_panel::{CompletionItem, CompletionPanel, MAX_ROWS};
use crate::list_menu::ListMenu;
use crate::live_preview::LivePreview;
use crate::rename_dialog::{self, LinkUpdate};
use crate::note_embeds::{Callbacks, Content, NoteEmbeds};
use crate::note_list_view::{self, NoteListSource};
use crate::note_view::{self, NoteTextView};
use crate::find_bar::FindBar;
use crate::search_bar::SearchBar;
use crate::search_worker::{SearchRequest, SearchResponse, SearchWorker};
use crate::sidebar_view::{self, FolderChoice, SidebarSource};
use crate::space_switcher::SpaceSwitcher;
use crate::toolbar::{self, ToolbarDelegate};
use crate::{probe, strings, window};

/// What the window shows: the last snapshot and the selected sidebar row.
struct State {
    selection: SidebarItem,
    rows: Arc<Vec<NoteRow>>,
    rules: Arc<HashMap<String, FolderRules>>,
    folders: Arc<FolderNode>,
    /// Every folder of the vault, empty ones too (the snapshot's list).
    folder_paths: Arc<Vec<String>>,
    tags: Arc<Vec<TagNode>>,
    /// The decks of the notes' cards as a tree (ADR 0037), each with its cards to review.
    decks: Arc<Vec<TagNode>>,
    /// Notes without any tag (the "Untagged" row), and every tag with its note, a note's
    /// together (a space's tags count from them, ADR 0030): the last snapshot's.
    untagged: Arc<HashSet<String>>,
    tagged: Arc<Vec<(String, String)>>,
    /// Each space's tags and untagged notes, as the last snapshot counted them.
    space_tags: Arc<Vec<SpaceTags>>,
    index_dir: Option<PathBuf>,
    vault: Option<PathBuf>,
    /// List settings of tag views: tags have no folder and no `.knotes.json`; kept for the session.
    tag_settings: HashMap<String, ListSettings>,
    history: History,
    /// Read-only index connection, opened on first need (the index exists after the first snapshot).
    reader: Option<SqliteIndex>,
    /// Link resolution of the last snapshot (ADR 0013); `None` until the first one.
    links: Option<Arc<LinkTargets>>,
    /// Every vault file of the last snapshot, to resolve links as they will be after a move.
    catalog: Arc<Vec<CatalogEntry>>,
    /// Title search of the last snapshot, for link completion.
    titles: Option<Arc<TitleIndex>>,
    /// Folders or tags under "All notes".
    sidebar_mode: SidebarMode,
    /// Text in the search field over the list; the list shows its results when not empty.
    search: String,
    /// The vault's spaces (ADR 0030), the window's (none: the whole vault) and its folders.
    spaces: Arc<Spaces>,
    space: Option<String>,
    space_folders: Option<SpaceFolders>,
    /// The tags the sidebar lists under «Решта бази» (none there in the space).
    rest_tags: HashSet<String>,
    /// The recently deleted, the newest first (ADR 0032).
    trash: Arc<Vec<TrashEntry>>,
    /// What is kept from the AI (ADR 0036).
    ai: Arc<knotes_spaces::KeptFromAi>,
}

impl State {
    /// The space's folders again, after a snapshot or a change of space; none when the vault
    /// no longer has the space.
    fn fit_space(&mut self) {
        if self.space.as_ref().is_some_and(|id| self.spaces.get(id).is_none()) {
            self.space = None;
        }
        self.space_folders = self.space.as_ref().map(|id| SpaceFolders::new(&self.folders, &self.spaces, id));
    }

}

/// Autosave runs after this long without edits (ADR 0011).
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);
/// The pause in typing after which the outline is listed again.
const OUTLINE_DELAY: Duration = Duration::from_millis(150);

/// The note open in the editor.
struct Editing {
    path: String,
    document: Document,
    /// A file not in UTF-8: shown as well as it reads and never written, which would replace
    /// its bytes.
    read_only: bool,
}

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
    pub list: Retained<NoteListSource>,
    pub table: Retained<NSTableView>,
    pub list_menu: ListMenu,
    pub note_text: Retained<NoteTextView>,
    /// Where the open document is in the vault, at the bottom of the note column.
    pub path_bar: crate::path_bar::PathBar,
    /// A deck's review, over the note column (ADR 0037), and the review under way.
    review_view: crate::review_view::ReviewView,
    review: RefCell<Option<cards::Review>>,
    /// The deck the last card was made in, offered for the next.
    last_deck: RefCell<String>,
    /// The search row over the note list.
    pub search_bar: SearchBar,
    /// Find in the open note (Cmd+F), over the note.
    pub find_bar: FindBar,
    /// A PDF opened in place of the note (ADR 0022), and its vault path. The view is made for
    /// each PDF and let go with it: PDFKit keeps every page it drew while the view lives.
    pdf_view: RefCell<Option<PdfViewer>>,
    pdf_path: RefCell<Option<String>>,
    /// The recently deleted entry shown, read where it is kept (ADR 0032): nothing open then.
    previewing: RefCell<Option<TrashEntry>>,
    /// The occurrences found in the note (UTF-16) and the one selected.
    find_matches: RefCell<Vec<Found>>,
    /// Set to call off the search under way in the PDF shown.
    pdf_find: RefCell<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    find_current: Cell<Option<usize>>,
    /// Delegate of `note_text`, kept alive here.
    live_preview: Retained<LivePreview>,
    /// The notes the open note embeds, in views of their own over it (ADR 0034).
    embeds: NoteEmbeds,
    /// The chat with the model (ADR 0027), in the right panel.
    pub chat: Rc<ChatPanel>,
    /// The outline of the document open, in the right panel beside the chat.
    pub outline_panel: OutlinePanel,
    /// The connections of the document open, in the right panel (ADR 0026), the document they
    /// are of, and the gathering under way (set to call it off).
    pub connections_panel: ConnectionsPanel,
    connections_of: RefCell<Option<String>>,
    connections_job: RefCell<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Said in the note column while no document is open.
    no_document: Retained<NSTextField>,
    /// The documents open as tabs (ADR 0024), and whether the last launch's came back.
    tabs: RefCell<Tabs>,
    tabs_restored: Cell<bool>,
    /// Whether the last launch's space came back (on the first snapshot).
    space_restored: Cell<bool>,
    /// The sidebar waits while a folder's name is edited: rebuilt when it ends.
    sidebar_waiting: Cell<bool>,
    /// The next document opened goes in a new tab (the list's menu asked for it).
    new_tab_next: Cell<bool>,
    /// The next document is opened from the note (a link or a preview clicked in it).
    from_note: Cell<bool>,
    /// What the right panel shows.
    panel_mode: Cell<PanelMode>,
    /// The document the outline shows (a note's or a PDF's path), and whether an edit's
    /// refresh of it is due.
    outline_of: RefCell<Option<String>>,
    outline_due: Cell<bool>,
    /// The heading last clicked in the outline and how far the document was scrolled to it:
    /// it stays highlighted until the document scrolls on, though it may not reach the top.
    outline_jump: Cell<Option<(usize, f64)>>,
    /// Kept alive here: the toolbar holds its delegate weakly.
    toolbar_delegate: Retained<ToolbarDelegate>,
    /// The space switcher at the sidebar's foot (ADR 0030).
    space_switcher: Retained<SpaceSwitcher>,
    state: RefCell<State>,
    /// Set while the window selects a sidebar or list row itself, so selection callbacks ignore it.
    highlighting: Cell<bool>,
    editing: RefCell<Option<Editing>>,
    /// Bumped on every edit: a scheduled autosave runs only if no edit came after it.
    edits: Cell<u64>,
    /// The same for edits in embedded notes (ADR 0034).
    embed_edits: Cell<u64>,
    /// A save failed and was told; told again only after one has worked.
    save_failed: Cell<bool>,
    /// The last indexing failure told, so the same one is not told again.
    index_error: RefCell<Option<String>>,
    this: RefCell<Weak<Self>>,
    completion: CompletionPanel,
    /// UTF-16 start of the text an accepted completion replaces, and what it completes.
    completion_from: Cell<usize>,
    completing: Cell<Completing>,
    /// Where the list shown began (UTF-16: a link's `[[`, a tag's name), and the one Esc
    /// closed: it stays closed while typing on.
    completion_anchor: Cell<usize>,
    completion_dismissed: Cell<Option<usize>>,
    /// The text view the list is for: an embedded note's (ADR 0034), else the note's own.
    completion_view: RefCell<Option<Retained<NSTextView>>>,
    /// Set while a completion is inserted, so the edit does not reopen the list.
    accepting: Cell<bool>,
    /// List search: the background thread, the latest request and the results shown.
    search_worker: RefCell<Option<SearchWorker>>,
    search_generation: Cell<u64>,
    search_results: RefCell<Option<SearchResponse>>,
    /// The paths a search of the shown list covers (`None`: all notes).
    search_scope: RefCell<Option<Vec<String>>>,
    /// In a space, the list's notes in it, which search by meaning finds apart (ADR 0030).
    search_space: RefCell<Option<Vec<String>>>,
    /// The sidebar's split view item, watched to hide the folders/tags switch with it.
    sidebar_item: Retained<NSSplitViewItem>,
    /// The list's column, hidden while a card is reviewed (ADR 0037).
    list_item: Retained<NSSplitViewItem>,
    collapse_observer: RefCell<Option<Retained<CollapseObserver>>>,
    /// The right panel's split view item, watched to hide the AI/outline switch with it.
    panel_item: Retained<NSSplitViewItem>,
    panel_observer: RefCell<Option<Retained<CollapseObserver>>>,
}

impl Drop for VaultWindow {
    fn drop(&mut self) {
        if let Some(observer) = self.collapse_observer.take() {
            observer.stop(&self.sidebar_item);
        }
        if let Some(observer) = self.panel_observer.take() {
            observer.stop(&self.panel_item);
        }
    }
}

impl VaultWindow {
    pub fn new(mtm: MainThreadMarker) -> Rc<Self> {
        let empty = Sidebar::new(&Default::default(), &[], SidebarMode::Folders, None, 0, &|_| false, None, 0);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        // The folder tree above the space switcher (ADR 0030).
        let (space_switcher, column) = SpaceSwitcher::column(mtm, &scroll);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&column);
        let list = NoteListSource::new(mtm);
        let (list_scroll, table) = note_list_view::table(mtm, &list);
        let list_vc = NSViewController::new(mtm);
        let search_bar = SearchBar::new(mtm, list_scroll);
        list_vc.setView(&search_bar.column);
        let (note_scroll, note_text) = note_view::note_view(mtm);
        let live_preview = LivePreview::new(mtm);
        note_text.setDelegate(Some(ProtocolObject::from_ref(&*live_preview)));
        live_preview.attach(&note_text);
        let clicks = live_preview.clone();
        note_text.on_click(move |view, index, cmd| clicks.click(view, index, cmd));
        let hovers = live_preview.clone();
        note_text.on_hover(move |index| hovers.clickable(index));
        let resizes = live_preview.clone();
        note_text.on_resize(move |view, line, width, done| resizes.resize_image(view, line, width, done));
        let files = live_preview.clone();
        note_text.on_image_file(move |view, line| files.image_file(view, line));
        let hovers = live_preview.clone();
        note_text.on_image_hover(move |view, line| hovers.hover_image(view, line));
        let pointers = live_preview.clone();
        note_text.on_pointer(move |view, point| {
            pointers.hover_table(view, point);
            pointers.hover_code(view, point);
        });
        let presses = live_preview.clone();
        note_text.on_press(move |view, point| presses.press(view, point));
        let scrolls = live_preview.clone();
        note_text.on_scroll_sideways(move |view, point, dx| scrolls.scroll_table(view, point, dx));
        let menus = live_preview.clone();
        note_text.on_context_menu(move |view, point, menu| {
            let menu = menus.link_context_menu(view, point, menu);
            let menu = menus.table_context_menu(view, point, menu);
            // «Зробити картку» first, with something selected (ADR 0037); sent up to the app.
            if let Some(menu) = menu.as_ref().filter(|_| view.selectedRange().length > 0) {
                let mtm = MainThreadMarker::from(view);
                // SAFETY: no target: the action goes up the responder chain to the app's delegate.
                let item = unsafe { objc2_app_kit::NSMenuItem::initWithTitle_action_keyEquivalent(<objc2_app_kit::NSMenuItem as objc2::MainThreadOnly>::alloc(mtm), &NSString::from_str(strings::MAKE_CARD), Some(sel!(makeCard:)), &NSString::new()) };
                menu.insertItem_atIndex(&item, 0);
                menu.insertItem_atIndex(&objc2_app_kit::NSMenuItem::separatorItem(mtm), 1);
            }
            menu
        });
        // Weak: the text view keeps its callbacks, and the text view is the live preview's.
        let preview = live_preview.clone();
        let text = objc2::rc::Weak::new(&*note_text);
        note_text.on_appearance(move || {
            if let Some(text) = text.load() {
                preview.appearance_changed(&text);
            }
        });
        let note_vc = NSViewController::new(mtm);
        let find_bar = FindBar::new(mtm, note_scroll.clone());
        note_vc.setView(&find_bar.column);
        let no_document = NSTextField::labelWithString(&NSString::from_str(strings::NO_DOCUMENT), mtm);
        no_document.setFont(Some(&objc2_app_kit::NSFont::systemFontOfSize(15.0)));
        no_document.setTextColor(Some(&objc2_app_kit::NSColor::tertiaryLabelColor()));
        no_document.setTranslatesAutoresizingMaskIntoConstraints(false);
        find_bar.column.addSubview(&no_document);
        let path_bar = crate::path_bar::PathBar::new(mtm);
        find_bar.column.addSubview(&path_bar.view);
        for c in [
            path_bar.view.leadingAnchor().constraintEqualToAnchor(&find_bar.column.leadingAnchor()),
            path_bar.view.trailingAnchor().constraintEqualToAnchor(&find_bar.column.trailingAnchor()),
            path_bar.view.bottomAnchor().constraintEqualToAnchor(&find_bar.column.bottomAnchor()),
        ] {
            c.setActive(true);
        }
        find_bar.set_bottom(crate::path_bar::HEIGHT);
        let review_view = crate::review_view::ReviewView::new(mtm);
        find_bar.column.addSubview(&review_view.view);
        for c in [
            review_view.view.topAnchor().constraintEqualToAnchor(&find_bar.column.topAnchor()),
            review_view.view.leadingAnchor().constraintEqualToAnchor(&find_bar.column.leadingAnchor()),
            review_view.view.trailingAnchor().constraintEqualToAnchor(&find_bar.column.trailingAnchor()),
            review_view.view.bottomAnchor().constraintEqualToAnchor(&find_bar.column.bottomAnchor()),
        ] {
            c.setActive(true);
        }
        no_document.centerXAnchor().constraintEqualToAnchor(&find_bar.column.centerXAnchor()).setActive(true);
        no_document.centerYAnchor().constraintEqualToAnchor(&find_bar.column.centerYAnchor()).setActive(true);
        let chat = ChatPanel::new(mtm, knotes_session::app_data_dir());
        let outline_panel = OutlinePanel::new(mtm);
        let connections_panel = ConnectionsPanel::new(mtm);
        let panel = NSView::new(mtm);
        for view in [&chat.view, &outline_panel.view, &connections_panel.view] {
            view.setTranslatesAutoresizingMaskIntoConstraints(false);
            panel.addSubview(view);
            for c in [
                view.topAnchor().constraintEqualToAnchor(&panel.topAnchor()),
                view.bottomAnchor().constraintEqualToAnchor(&panel.bottomAnchor()),
                view.leadingAnchor().constraintEqualToAnchor(&panel.leadingAnchor()),
                view.trailingAnchor().constraintEqualToAnchor(&panel.trailingAnchor()),
            ] {
                c.setActive(true);
            }
        }
        outline_panel.view.setHidden(true);
        connections_panel.view.setHidden(true);
        let chat_vc = NSViewController::new(mtm);
        chat_vc.setView(&panel);
        let (window, split) = window::build(mtm, &sidebar_vc, &list_vc, &note_vc, &chat_vc);
        let list_menu = ListMenu::new(mtm);
        let toolbar = ToolbarDelegate::new(mtm, split.splitView(), list_menu.menu.clone());
        search_bar.set_target(toolbar.as_ref());
        search_bar.set_meaning_on(crate::meaning::model().is_some());
        toolbar::install(&window, &toolbar, &search_bar.field);
        let state = State {
            selection: SidebarItem::AllNotes,
            rows: Arc::new(Vec::new()),
            rules: Arc::new(HashMap::new()),
            folders: Arc::new(FolderNode::default()),
            folder_paths: Arc::new(Vec::new()),
            tags: Arc::new(Vec::new()),
            decks: Arc::default(),
            untagged: Arc::default(),
            tagged: Arc::default(),
            space_tags: Arc::default(),
            index_dir: None,
            vault: None,
            tag_settings: HashMap::new(),
            history: History::default(),
            reader: None,
            links: None,
            catalog: Arc::new(Vec::new()),
            titles: None,
            sidebar_mode: SidebarMode::Folders,
            search: String::new(),
            spaces: Arc::new(Spaces::default()),
            space: None,
            space_folders: None,
            rest_tags: HashSet::new(),
            trash: Arc::new(Vec::new()),
            ai: Arc::default(),
        };
        let this = Rc::new(Self {
            window,
            sidebar,
            outline,
            list,
            table,
            list_menu,
            note_text,
            path_bar,
            review_view,
            review: RefCell::new(None),
            last_deck: RefCell::new(String::new()),
            search_bar,
            find_bar,
            pdf_view: RefCell::new(None),
            pdf_path: RefCell::new(None),
            previewing: RefCell::new(None),
            find_matches: RefCell::new(Vec::new()),
            pdf_find: RefCell::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))),
            find_current: Cell::new(None),
            live_preview,
            embeds: NoteEmbeds::default(),
            chat,
            outline_panel,
            connections_panel,
            connections_of: RefCell::new(None),
            connections_job: RefCell::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))),
            no_document,
            tabs: RefCell::new(Tabs::default()),
            tabs_restored: Cell::new(false),
            space_restored: Cell::new(false),
            sidebar_waiting: Cell::new(false),
            new_tab_next: Cell::new(false),
            from_note: Cell::new(false),
            panel_mode: Cell::new(PanelMode::Ai),
            outline_of: RefCell::new(None),
            outline_due: Cell::new(false),
            outline_jump: Cell::new(None),
            toolbar_delegate: toolbar,
            space_switcher,
            state: RefCell::new(state),
            highlighting: Cell::new(false),
            editing: RefCell::new(None),
            edits: Cell::new(0),
            embed_edits: Cell::new(0),
            save_failed: Cell::new(false),
            index_error: RefCell::new(None),
            this: RefCell::new(Weak::new()),
            completion: CompletionPanel::new(mtm),
            completion_from: Cell::new(0),
            completing: Cell::new(Completing::Link),
            completion_anchor: Cell::new(0),
            completion_dismissed: Cell::new(None),
            completion_view: RefCell::new(None),
            accepting: Cell::new(false),
            search_worker: RefCell::new(None),
            search_generation: Cell::new(0),
            search_results: RefCell::new(None),
            search_scope: RefCell::new(None),
            search_space: RefCell::new(None),
            sidebar_item: split.splitViewItems().objectAtIndex(0),
            list_item: split.splitViewItems().objectAtIndex(1),
            collapse_observer: RefCell::new(None),
            panel_item: split.splitViewItems().objectAtIndex(3),
            panel_observer: RefCell::new(None),
        });
        let toolbar = this.toolbar_delegate.clone();
        let list_item = split.splitViewItems().objectAtIndex(1);
        let split_view = split.splitView();
        let list_width = Cell::new(None::<f64>);
        let observer = CollapseObserver::watch(mtm, &this.sidebar_item, move |collapsed| {
            toolbar.set_sidebar_shown(!collapsed);
            let list = list_item.viewController(mtm).view();
            // With the sidebar collapsed, the traffic lights, the sidebar button, the title and
            // the list options all sit over the list column: it must be wide enough for the
            // toolbar separator to stay on the list/note boundary. The list's own width comes
            // back with the sidebar.
            if collapsed {
                list_width.set(Some(list.frame().size.width));
                list_item.setMinimumThickness(COLLAPSED_LIST_MIN);
            } else {
                list_item.setMinimumThickness(COLUMNS[1].0);
                if let Some(width) = list_width.take() {
                    let (list, split_view) = (MainThreadBound::new(list, mtm), MainThreadBound::new(split_view.clone(), mtm));
                    // Once the sidebar is back in place.
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                        let list = list.get(mtm);
                        split_view.get(mtm).setPosition_ofDividerAtIndex(list.frame().origin.x + width, 1);
                    });
                }
            }
        });
        *this.collapse_observer.borrow_mut() = Some(observer);
        let toolbar = this.toolbar_delegate.clone();
        *this.panel_observer.borrow_mut() = Some(CollapseObserver::watch(mtm, &this.panel_item, move |collapsed| toolbar.set_panel_shown(!collapsed)));
        *this.this.borrow_mut() = Rc::downgrade(&this);
        this.show_note_column();
        let weak_chat: Weak<Self> = Rc::downgrade(&this);
        this.chat.on_open_link(move |target| {
            if let Some(window) = weak_chat.upgrade() {
                window.open_chat_link(&target);
            }
        });
        let weak_open: Weak<Self> = Rc::downgrade(&this);
        this.connections_panel.on_open(move |path, range| {
            if let Some(window) = weak_open.upgrade() {
                window.open_connection(&path, range);
            }
        });
        let weak_follow: Weak<Self> = Rc::downgrade(&this);
        this.connections_panel.on_follow(move |label, resolved| {
            if let Some(window) = weak_follow.upgrade() {
                window.follow_connection(&label, resolved);
            }
        });
        let weak_more: Weak<Self> = Rc::downgrade(&this);
        this.connections_panel.on_more(move || {
            if let Some(window) = weak_more.upgrade() {
                window.connections_panel.relist();
            }
        });
        let weak_make: Weak<Self> = Rc::downgrade(&this);
        this.connections_panel.on_make_link(move |source, range| {
            if let Some(window) = weak_make.upgrade() {
                window.make_connection_link(&source, range);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.outline_panel.on_choose(move |heading| {
            if let Some(window) = weak.upgrade() {
                window.go_to_heading(heading);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_scroll(move || {
            if let Some(window) = weak.upgrade() {
                window.outline_follow();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_link(move |link| {
            if let Some(window) = weak.upgrade() {
                window.follow_link(link);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.note_text.on_layout(move || {
            if let Some(window) = weak.upgrade()
                && window.embeds.position(&window.note_text, &window.live_preview)
            {
                // The column's width changed: measured again after this layout, not inside it.
                let Some(mtm) = MainThreadMarker::new() else { return };
                let target = MainThreadBound::new(Rc::downgrade(&window), mtm);
                DispatchQueue::main().exec_async(move || {
                    let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                    if let Some(window) = target.get(mtm).upgrade() {
                        window.sync_embeds(false);
                    }
                });
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_change(move || {
            if let Some(window) = weak.upgrade() {
                window.edited();
                window.sync_embeds(false);
                window.update_completion();
                window.outline_later();
                // An edit drops the highlights: the occurrences are found again, in place.
                if window.find_bar.is_shown() {
                    window.find_text(&window.find_bar.text(), false);
                }
            }
        });
        let (text_weak, step_weak, close_weak) = (Rc::downgrade(&this), Rc::downgrade(&this), Rc::downgrade(&this));
        this.find_bar.on_events(
            move |text| {
                if let Some(window) = text_weak.upgrade() {
                    window.find_text(&text, true);
                }
            },
            move |forward| {
                if let Some(window) = step_weak.upgrade() {
                    window.find_step(forward);
                }
            },
            move || {
                if let Some(window) = close_weak.upgrade() {
                    window.close_find();
                }
            },
        );
        let weak: Weak<Self> = Rc::downgrade(&this);
        let pastes: Weak<Self> = Rc::downgrade(&this);
        this.note_text.on_paste(move |pasteboard, at| pastes.upgrade().is_some_and(|window| window.paste_images(pasteboard, at)));
        this.note_text.on_find(move |action| {
            if let Some(window) = weak.upgrade() {
                window.find_action(action);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_cursor(move || {
            if let Some(window) = weak.upgrade() {
                window.update_completion();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_command(move |command| weak.upgrade().is_some_and(|window| window.completion_command(command)));
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_select(move |item| {
            if let Some(window) = weak.upgrade() {
                window.select(item);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.completion.on_click(move |row| {
            if let Some(window) = weak.upgrade() {
                window.completion.select(row);
                window.accept_completion();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_rename(move |path, name| {
            if let Some(window) = weak.upgrade() {
                window.rename_note(&path, &name);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_move(move |path| {
            if let Some(window) = weak.upgrade() {
                window.move_note_to_chosen_folder(&path);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        // The space switcher (ADR 0030): a space chosen, or its settings asked for.
        let weak_space: Weak<Self> = Rc::downgrade(&this);
        this.space_switcher.on_space(move |id| {
            if let Some(window) = weak_space.upgrade() {
                window.set_space(id);
            }
        });
        let weak_settings: Weak<Self> = Rc::downgrade(&this);
        this.space_switcher.on_settings(move || {
            if let Some(window) = weak_settings.upgrade() {
                crate::settings_window::show_spaces(MainThreadMarker::from(&*window.window), window.vault());
            }
        });
        let weak_new_folder: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_new_folder(move |parent| {
            if let Some(window) = weak_new_folder.upgrade() {
                window.new_folder(Some(parent));
            }
        });
        // The recently deleted (ADR 0032): into them, back out, or gone for good.
        let weak_delete: Weak<Self> = Rc::downgrade(&this);
        this.list.on_delete(move |path| {
            if let Some(window) = weak_delete.upgrade() {
                window.delete(&path);
            }
        });
        let weak_delete_folder: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_delete_folder(move |folder| {
            if let Some(window) = weak_delete_folder.upgrade() {
                window.delete_folder(&folder);
            }
        });
        // Kept from the AI, or shown to it again (ADR 0036).
        let weak_ai: Weak<Self> = Rc::downgrade(&this);
        this.list.on_ai(move |path| {
            if let Some(window) = weak_ai.upgrade() {
                window.toggle_ai(&path);
            }
        });
        let weak_folder_ai: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_folder_ai(move |folder| {
            if let Some(window) = weak_folder_ai.upgrade() {
                window.toggle_folder_ai(&folder);
            }
        });
        let weak_restore: Weak<Self> = Rc::downgrade(&this);
        this.list.on_restore(move |id| {
            if let Some(window) = weak_restore.upgrade() {
                window.restore(&id);
            }
        });
        let weak_purge: Weak<Self> = Rc::downgrade(&this);
        this.list.on_purge(move |id| {
            if let Some(window) = weak_purge.upgrade() {
                window.purge(Some(&id));
            }
        });
        let weak_empty: Weak<Self> = Rc::downgrade(&this);
        this.list.on_empty(move || {
            if let Some(window) = weak_empty.upgrade() {
                window.purge(None);
            }
        });
        let weak_rename_end: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_rename_end(move || {
            if let Some(window) = weak_rename_end.upgrade() {
                window.rebuild_sidebar();
                let selection = window.state.borrow().selection.clone();
                window.highlight(&selection);
            }
        });
        let weak_folder_space: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_folder_space(move |folder, choice| {
            if let Some(window) = weak_folder_space.upgrade() {
                window.set_folder_space(&folder, choice);
            }
        });
        // The tabs/folders/tags switch; the choice lasts the session (every launch starts on the
        // folders).
        let weak_mode: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_sidebar_mode(move |mode| {
            if let Some(window) = weak_mode.upgrade() {
                window.set_sidebar_mode(mode);
            }
        });
        let weak_link_tab: Weak<Self> = Rc::downgrade(&this);
        this.live_preview.on_link_new_tab(move |link| {
            if let Some(window) = weak_link_tab.upgrade() {
                window.new_tab_next.set(true);
                window.follow_link(link);
                window.new_tab_next.set(false);
            }
        });
        let weak_new_tab: Weak<Self> = Rc::downgrade(&this);
        this.list.on_new_tab(move |path| {
            if let Some(window) = weak_new_tab.upgrade() {
                window.open_in_new_tab(&path);
            }
        });
        let weak_close: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_close(move |path| {
            if let Some(window) = weak_close.upgrade() {
                window.close_tab(&path);
            }
        });
        let weak_others: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_close_others(move |path| {
            if let Some(window) = weak_others.upgrade() {
                window.close_other_tabs(&path);
            }
        });
        this.sidebar.on_rename(move |folder, name| {
            if let Some(window) = weak.upgrade() {
                window.rename_folder(&folder, &name);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_drop(move |path, folder| {
            if let Some(window) = weak.upgrade() {
                window.move_note_to(&path, &folder);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_select(move |path| {
            if let Some(window) = weak.upgrade()
                && !window.highlighting.get()
                && window.open_path().as_deref() != Some(path.as_str())
            {
                window.open_note(&path, true);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list.on_select_deleted(move |id| {
            if let Some(window) = weak.upgrade()
                && !window.highlighting.get()
                && !window.shows_deleted(&id)
            {
                window.show_deleted(&id);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_new_note(move || {
            if let Some(window) = weak.upgrade() {
                window.new_note();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_toggle_search(move || {
            if let Some(window) = weak.upgrade() {
                window.toggle_search();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_search_case(move || {
            if let Some(window) = weak.upgrade()
                && !window.state.borrow().search.trim().is_empty()
            {
                window.request_search();
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_search_mode(move |mode| {
            if let Some(window) = weak.upgrade() {
                window.set_search_mode(mode);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_search(move |query| {
            if let Some(window) = weak.upgrade() {
                window.search(&query);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        let weak_review: Weak<Self> = Rc::downgrade(&this);
        this.review_view.on_asked(move |asked| {
            if let Some(window) = weak_review.upgrade() {
                window.review_asked(asked);
            }
        });
        let weak_path: Weak<Self> = Rc::downgrade(&this);
        this.path_bar.on_folder(move |folder| {
            if let Some(window) = weak_path.upgrade() {
                window.show_folder(&folder);
            }
        });
        let weak_done: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_title_done(move || {
            if let Some(window) = weak_done.upgrade()
                && window.open_path().is_some()
            {
                window.window.makeFirstResponder(Some(&window.note_text));
            }
        });
        let weak_title: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_note_title(move |name| {
            let Some(window) = weak_title.upgrade() else { return };
            let pdf = window.pdf_path.borrow().clone();
            let renamed = match (window.open_path(), pdf) {
                (Some(note), _) => {
                    let renamed = window.rename_note(&note, &name);
                    // Named by the user: no longer after its first line.
                    if renamed && let Some(editing) = window.editing.borrow_mut().as_mut() {
                        editing.document.named();
                    }
                    renamed
                }
                (None, Some(pdf)) => window.rename_pdf(&pdf, &name),
                (None, None) => false,
            };
            if !renamed {
                // Declined or impossible: the toolbar shows the name the document kept.
                window.update_navigation();
            }
        });
        this.toolbar_delegate.on_navigate(move |forward| {
            if let Some(window) = weak.upgrade() {
                window.navigate(forward);
            }
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.list_menu.on_choose(move |option| {
            if let Some(window) = weak.upgrade() {
                window.choose(option);
            }
        });
        this
    }

    /// Another model was chosen for search by meaning, or none (ADR 0029): the chat takes it, the
    /// search's switch shows whether there is one, and a search by meaning under way runs again
    /// (by file names, when search by meaning is off).
    pub fn meaning_changed(&self) {
        self.chat.meaning_changed();
        let on = crate::meaning::model().is_some();
        self.search_bar.set_meaning_on(on);
        let query = self.search_bar.field.stringValue().to_string();
        if parse_search(&query).0 == SearchMode::Meaning {
            if on { self.search(&query) } else { self.set_search_mode(SearchMode::Files) }
        }
    }

    pub fn set_vault(&self, vault: PathBuf, index_dir: PathBuf) {
        self.live_preview.set_vault(vault.clone());
        self.chat.set_vault(vault.clone(), index_dir.clone());
        let mut state = self.state.borrow_mut();
        state.vault = Some(vault);
        state.index_dir = Some(index_dir);
    }

    /// Applies a list options choice: saved in the folder's `.knotes.json` (only that field), or
    /// kept for the session for tag views.
    pub fn choose(&self, option: ListOption) {
        let mut state = self.state.borrow_mut();
        let selection = state.selection.clone();
        match session_settings_key(&selection) {
            Some(key) => {
                let current = state.tag_settings.get(&key).copied().unwrap_or_else(|| resolve_list_settings("", &state.rules));
                state.tag_settings.insert(key, current.with(option));
            }
            None => {
                let folder = match &selection {
                    SidebarItem::Folder(path) => path.clone(),
                    _ => String::new(),
                };
                let Some(vault) = state.vault.clone() else { return };
                drop(state);
                let Some(rules) = self.write_list_option(&FsVault::new(&vault), &folder, option) else { return };
                Arc::make_mut(&mut self.state.borrow_mut().rules).insert(folder, rules);
            }
        }
        self.refresh_list();
    }

    /// Writes `option` into the rules file of `folder` as it is on disk now, not as the last
    /// snapshot read it: what was written there since stays. A file that does not parse is not
    /// replaced (it would keep only the list's field; its spaces, `"ai": false` and the rest
    /// would go): the user is told, and nothing changes. The rules as written.
    fn write_list_option(&self, files: &FsVault, folder: &str, option: ListOption) -> Option<FolderRules> {
        let file = if folder.is_empty() { knotes_app::RULES_FILE.to_owned() } else { format!("{folder}/{}", knotes_app::RULES_FILE) };
        let parsed = files.read_rules(&[folder]).map_err(|e| e.to_string()).and_then(|found| {
            let text = found.into_iter().next().map(|(_, text)| text).filter(|text| !text.trim().is_empty());
            FolderRules::parse(text.as_deref().unwrap_or("{}"))
        });
        let mut rules = match parsed {
            Ok(rules) => rules,
            Err(e) => {
                self.tell_rules_not_written(&file, &e);
                return None;
            }
        };
        rules.set_option(option);
        if let Err(e) = files.write_rules(folder, &rules.to_json()) {
            self.tell_rules_not_written(&file, &e.to_string());
            return None;
        }
        Some(rules)
    }

    /// Says a list option was not saved, the rules file `file` being the reason (`error`).
    fn tell_rules_not_written(&self, file: &str, error: &str) {
        eprintln!("knotes: not written {file}: {error}");
        if probe::enabled() {
            println!("rules-not-written: {file}: {error}");
            return;
        }
        let alert = NSAlert::new(MainThreadMarker::from(&*self.window));
        alert.setMessageText(&NSString::from_str(strings::LIST_OPTION_NOT_SAVED));
        alert.setInformativeText(&NSString::from_str(&strings::rules_file_error(file, error)));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.beginSheetModalForWindow_completionHandler(&self.window, None);
    }

    /// Says indexing failed, once for each reason (a sync tried again after each change would
    /// say it each time).
    fn tell_index_error(&self, title: &str, details: &str) {
        if self.index_error.borrow().as_deref() == Some(details) {
            eprintln!("knotes: {title}: {details}");
            return;
        }
        *self.index_error.borrow_mut() = Some(details.to_owned());
        self.tell_failed(title, &details);
    }

    pub fn apply(&self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => {
                self.show(&snapshot);
                self.chat.index_changed();
            }
            Event::Error(e) => self.tell_index_error(strings::INDEX_FAILED, &strings::index_failed_details(&e)),
            Event::NotWatched(e) => self.tell_index_error(strings::NOT_WATCHED, &strings::not_watched_details(&e)),
        }
    }

    fn show(&self, snapshot: &Snapshot) {
        {
            let open = self.open_path();
            let mut state = self.state.borrow_mut();
            let paths: HashSet<&str> = snapshot.rows.iter().map(|r| r.path.as_str()).collect();
            // The open note stays even when this snapshot predates its file (just created or
            // renamed).
            state.history.retain(|p| paths.contains(p) || open.as_deref() == Some(p));
        }
        // Events lost (`RESCAN`): the note open and the embedded notes are looked at anyway.
        let rescan = snapshot.changed.iter().any(|p| p == knotes_vault::RESCAN);
        let changed = self.open_path().filter(|open| rescan || snapshot.changed.iter().any(|p| p == open));
        {
            let mut state = self.state.borrow_mut();
            state.rows = Arc::clone(&snapshot.rows);
            state.rules = Arc::clone(&snapshot.rules);
            state.folders = Arc::clone(&snapshot.folders);
            state.folder_paths = Arc::clone(&snapshot.folder_paths);
            state.tags = Arc::clone(&snapshot.tags);
            state.decks = Arc::new(knotes_app::deck_tree(&snapshot.decks));
            state.untagged = Arc::clone(&snapshot.untagged);
            state.tagged = Arc::clone(&snapshot.tagged);
            state.space_tags = Arc::clone(&snapshot.space_tags);
            state.catalog = Arc::clone(&snapshot.catalog);
            state.spaces = Arc::clone(&snapshot.spaces);
            state.trash = Arc::clone(&snapshot.trash);
            state.ai = Arc::clone(&snapshot.ai);
        }
        self.list.set_ai(Arc::clone(&snapshot.ai));
        self.sidebar.set_ai(Arc::clone(&snapshot.ai));
        if !self.space_restored.replace(true) {
            self.state.borrow_mut().space = self.saved_space();
        }
        self.state.borrow_mut().fit_space();
        self.show_spaces();
        let new_links = {
            let mut state = self.state.borrow_mut();
            let fresh = !state.links.as_ref().is_some_and(|l| Arc::ptr_eq(l, &snapshot.links));
            state.titles = Some(Arc::clone(&snapshot.titles));
            fresh.then(|| {
                state.links = Some(Arc::clone(&snapshot.links));
                Arc::clone(&snapshot.links)
            })
        };
        // Embedded notes read again when links resolve anew or their notes changed (ADR 0034).
        let embeds_changed = new_links.is_some() || rescan || snapshot.changed.iter().any(|p| self.embeds.shows(p));
        if let Some(links) = new_links {
            self.live_preview.set_links(&self.note_text, links);
        } else {
            self.live_preview.files_changed(&self.note_text, &snapshot.changed);
        }
        if embeds_changed {
            self.sync_embeds(true);
        }
        // Deleted documents lose their tabs.
        if let Some(vault) = self.vault() {
            self.tabs.borrow_mut().retain(|p| vault.join(p).is_file());
        }
        self.rebuild_sidebar();
        if !self.tabs_restored.replace(true) {
            self.restore_tabs();
        }
        // Other notes changed: links to the document open may have come or gone.
        self.refresh_connections();
        let mut selection = self.state.borrow().selection.clone();
        if !self.exists(&selection) {
            selection = SidebarItem::AllNotes;
            self.state.borrow_mut().selection = selection.clone();
        }
        self.highlight(&selection);
        self.refresh_list();
        if let Some(path) = changed {
            self.changed_on_disk(&path);
        }
        let pdf = self.pdf_path.borrow().clone();
        if let Some(pdf) = pdf.filter(|pdf| rescan || snapshot.changed.iter().any(|p| p == pdf)) {
            self.pdf_changed_on_disk(&pdf);
        }
        // The note open is gone from the disk (deleted or moved outside Knotes): it closes, its
        // unwritten edits beside where it was. Asked of the disk, not of the snapshot, which may
        // predate a file just made or renamed.
        if let (Some(path), Some(vault)) = (self.open_path(), self.vault())
            && !vault.join(&path).exists()
        {
            self.note_vanished();
        }
        self.update_navigation();
        if probe::enabled() && !snapshot.rows.is_empty() && probe::act_once() {
            self.act_as_probe_asks();
        }
    }
}

/// The name a document shows: a note's without `.md`, a PDF's without `.pdf`, neither with its
/// folder.
pub(crate) fn document_name(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    [".md", ".pdf"].iter().find_map(|ext| name.len().checked_sub(ext.len()).filter(|&at| name[at..].eq_ignore_ascii_case(ext)).map(|at| &name[..at])).unwrap_or(name)
}

pub fn split_items(
    sidebar: &NSViewController,
    list: &NSViewController,
    note: &NSViewController,
    chat: &NSViewController,
) -> [Retained<NSSplitViewItem>; 4] {
    // The sidebar starts under the toolbar and its row, as the other columns do (owner's request,
    // after DEVONthink's): at full height it ran up beside the toolbar's buttons, and the row
    // under the toolbar began only where it ended.
    let sidebar = NSSplitViewItem::sidebarWithViewController(sidebar);
    sidebar.setAllowsFullHeightLayout(false);
    [
        sidebar,
        NSSplitViewItem::contentListWithViewController(list),
        NSSplitViewItem::splitViewItemWithViewController(note),
        NSSplitViewItem::inspectorWithViewController(chat),
    ]
}

/// Minimum and maximum width and holding priority per column: when the window resizes, the
/// note column stretches and the others keep their width. Explicit maximums: the inspector
/// role caps itself at 270 points otherwise.
/// Narrowest list column while the sidebar is collapsed (see `VaultWindow::new`).
const COLLAPSED_LIST_MIN: f64 = 400.0;

const COLUMNS: [(f64, f64, f32); 4] = [(200.0, 480.0, 260.0), (260.0, 640.0, 255.0), (360.0, f64::MAX, 250.0), (260.0, 640.0, 255.0)];

pub fn split_controller(
    sidebar: &NSViewController,
    list: &NSViewController,
    note: &NSViewController,
    chat: &NSViewController,
    mtm: MainThreadMarker,
) -> Retained<NSSplitViewController> {
    let split = NSSplitViewController::new(mtm);
    for (item, (min, max, holding)) in split_items(sidebar, list, note, chat).iter().zip(COLUMNS) {
        item.setMinimumThickness(min);
        if max < f64::MAX {
            item.setMaximumThickness(max);
        }
        item.setHoldingPriority(holding);
        split.addSplitViewItem(item);
    }
    split
}
