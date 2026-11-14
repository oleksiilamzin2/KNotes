//! A window showing one vault: owns the AppKit objects and applies session events.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};

use knotes_app::{Moves, Relinks, embed_images, list_scope, pasted_image_name, plan_relinks, 
    FolderNode, FolderRules, History, ListFilter, ListOption, ListSettings, Outline, OutlineEntry, PanelMode, SearchMode, Sidebar, SidebarItem, SidebarMode, Tabs, AfterClose, TabPlace, TagNode, parse_search,
    search_list, with_search_mode, build_note_list, new_notes_folder,
    note_file_stem, resolve_list_settings, unique_note_path,
};
use knotes_core::{AttachmentStore, CatalogEntry, FileKind, FolderRulesStore, Link, LinkKind, LinkResolver, NoteMover, NoteParser, NoteRow, NoteWriter, TagIndex, VaultReader};
use knotes_md::MarkdownParser;
use knotes_core::TitleSearch;
use knotes_editor::{DiskChange, Document, LinkCompletion, conflict_path, link_completion, note_headings, subpath_offset, utf16_offsets};
use knotes_index::{LinkTargets, SqliteIndex};
use knotes_search::TitleIndex;
use knotes_session::{Event, Snapshot};
use knotes_vault::FsVault;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2::runtime::{ProtocolObject, Sel};
use objc2::sel;
use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF, NSModalResponse, NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSTextField, NSTextInputClient, NSView, NSViewController, NSWindow, NSWorkspace};
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
use crate::note_list_view::{self, NoteListSource};
use crate::note_view::{self, NoteTextView};
use crate::find_bar::FindBar;
use crate::search_bar::SearchBar;
use crate::search_worker::{SearchRequest, SearchResponse, SearchWorker};
use crate::sidebar_view::{self, SidebarSource};
use crate::toolbar::{self, ToolbarDelegate};
use crate::{probe, strings, window};

/// What the window shows: the last snapshot and the selected sidebar row.
struct State {
    selection: SidebarItem,
    rows: Arc<Vec<NoteRow>>,
    rules: Arc<HashMap<String, FolderRules>>,
    folders: Arc<FolderNode>,
    tags: Arc<Vec<TagNode>>,
    /// Notes without any tag (the "Untagged" row).
    untagged: usize,
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
}

/// Autosave runs after this long without edits (ADR 0011).
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);
/// The pause in typing after which the outline is listed again.
const OUTLINE_DELAY: Duration = Duration::from_millis(150);
/// The vault's tabs, beside its index (ADR 0024).
const TABS_FILE: &str = "tabs.json";

/// The note open in the editor.
struct Editing {
    path: String,
    document: Document,
}

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
    pub list: Retained<NoteListSource>,
    pub table: Retained<NSTableView>,
    pub list_menu: ListMenu,
    pub note_text: Retained<NoteTextView>,
    /// The search row over the note list.
    pub search_bar: SearchBar,
    /// Find in the open note (Cmd+F), over the note.
    pub find_bar: FindBar,
    /// A PDF opened in place of the note (ADR 0022), and its vault path. The view is made for
    /// each PDF and let go with it: PDFKit keeps every page it drew while the view lives.
    pdf_view: RefCell<Option<PdfViewer>>,
    pdf_path: RefCell<Option<String>>,
    /// The occurrences found in the note (UTF-16) and the one selected.
    find_matches: RefCell<Vec<(usize, usize)>>,
    /// Set to call off the search under way in the PDF shown.
    pdf_find: RefCell<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    find_current: Cell<Option<usize>>,
    /// Delegate of `note_text`, kept alive here.
    live_preview: Retained<LivePreview>,
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
    state: RefCell<State>,
    /// Set while the window selects a sidebar or list row itself, so selection callbacks ignore it.
    highlighting: Cell<bool>,
    editing: RefCell<Option<Editing>>,
    /// Bumped on every edit: a scheduled autosave runs only if no edit came after it.
    edits: Cell<u64>,
    this: RefCell<Weak<Self>>,
    completion: CompletionPanel,
    /// UTF-16 start of the text an accepted completion replaces.
    completion_from: Cell<usize>,
    /// The `[[` (UTF-16 location) whose list Esc closed: it stays closed while typing on.
    completion_dismissed: Cell<Option<usize>>,
    /// Set while a completion is inserted, so the edit does not reopen the list.
    accepting: Cell<bool>,
    /// List search: the background thread, the latest request and the results shown.
    search_worker: RefCell<Option<SearchWorker>>,
    search_generation: Cell<u64>,
    search_results: RefCell<Option<SearchResponse>>,
    /// The paths a search of the shown list covers (`None`: all notes).
    search_scope: RefCell<Option<Vec<String>>>,
    /// The sidebar's split view item, watched to hide the folders/tags switch with it.
    sidebar_item: Retained<NSSplitViewItem>,
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
        let empty = Sidebar::new(&Default::default(), &[], SidebarMode::Folders, None, 0, &|_| false);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&scroll);
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
            menus.table_context_menu(view, point, menu)
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
        toolbar::install(&window, &toolbar);
        let state = State {
            selection: SidebarItem::AllNotes,
            rows: Arc::new(Vec::new()),
            rules: Arc::new(HashMap::new()),
            folders: Arc::new(FolderNode::default()),
            tags: Arc::new(Vec::new()),
            untagged: 0,
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
        };
        let this = Rc::new(Self {
            window,
            sidebar,
            outline,
            list,
            table,
            list_menu,
            note_text,
            search_bar,
            find_bar,
            pdf_view: RefCell::new(None),
            pdf_path: RefCell::new(None),
            find_matches: RefCell::new(Vec::new()),
            pdf_find: RefCell::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))),
            find_current: Cell::new(None),
            live_preview,
            chat,
            outline_panel,
            connections_panel,
            connections_of: RefCell::new(None),
            connections_job: RefCell::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))),
            no_document,
            tabs: RefCell::new(Tabs::default()),
            tabs_restored: Cell::new(false),
            new_tab_next: Cell::new(false),
            from_note: Cell::new(false),
            panel_mode: Cell::new(PanelMode::Ai),
            outline_of: RefCell::new(None),
            outline_due: Cell::new(false),
            outline_jump: Cell::new(None),
            toolbar_delegate: toolbar,
            state: RefCell::new(state),
            highlighting: Cell::new(false),
            editing: RefCell::new(None),
            edits: Cell::new(0),
            this: RefCell::new(Weak::new()),
            completion: CompletionPanel::new(mtm),
            completion_from: Cell::new(0),
            completion_dismissed: Cell::new(None),
            accepting: Cell::new(false),
            search_worker: RefCell::new(None),
            search_generation: Cell::new(0),
            search_results: RefCell::new(None),
            search_scope: RefCell::new(None),
            sidebar_item: split.splitViewItems().objectAtIndex(0),
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
        this.live_preview.on_change(move || {
            if let Some(window) = weak.upgrade() {
                window.edited();
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
        let weak_title: Weak<Self> = Rc::downgrade(&this);
        this.toolbar_delegate.on_note_title(move |name| {
            let Some(window) = weak_title.upgrade() else { return };
            let pdf = window.pdf_path.borrow().clone();
            let renamed = match (window.open_path(), pdf) {
                (Some(note), _) => window.rename_note(&note, &name),
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

    /// Folders or tags under "All notes" (the toolbar switch); the list keeps what it shows.
    pub fn set_sidebar_mode(&self, mode: SidebarMode) {
        self.state.borrow_mut().sidebar_mode = mode;
        self.toolbar_delegate.set_sidebar_mode(mode);
        self.rebuild_sidebar();
        let selection = self.state.borrow().selection.clone();
        self.highlight(&selection);
    }

    /// The AI or the outline in the right panel (the toolbar switch).
    pub fn set_panel_mode(&self, mode: PanelMode) {
        self.toolbar_delegate.set_panel_mode(mode);
        self.show_panel(mode);
    }

    fn show_panel(&self, mode: PanelMode) {
        self.panel_mode.set(mode);
        self.chat.view.setHidden(mode != PanelMode::Ai);
        self.outline_panel.view.setHidden(mode != PanelMode::Outline);
        self.connections_panel.view.setHidden(mode != PanelMode::Links);
        self.refresh_panel();
    }

    /// The right panel's outline or connections for the document open.
    fn refresh_panel(&self) {
        self.chat.set_document(self.pdf_path.borrow().clone().or_else(|| self.open_path()));
        self.refresh_outline();
        self.refresh_connections();
    }

    /// Lists the connections of the document open, when the panel shows them (ADR 0026): its
    /// own links now, from the editor's text; the notes linking to it (from the index's
    /// backlinks) and naming it (from a full-text search of its name), with their lines, once
    /// gathered on another thread. Another document's list is cleared at once; the same
    /// document's stays until the new one comes.
    fn refresh_connections(&self) {
        if self.panel_mode.get() != PanelMode::Links {
            return;
        }
        self.connections_job.borrow().store(true, std::sync::atomic::Ordering::Relaxed);
        let document = self.pdf_path.borrow().clone().or_else(|| self.open_path());
        let (vault, index_dir, links) = {
            let state = self.state.borrow();
            (state.vault.clone(), state.index_dir.clone(), state.links.clone())
        };
        let (Some(document), Some(vault), Some(index_dir), Some(links), Some(mtm)) = (document, vault, index_dir, links, MainThreadMarker::new()) else {
            *self.connections_of.borrow_mut() = None;
            self.connections_panel.show(Connections::default());
            return;
        };
        let outgoing = self.outgoing_of(&document);
        if self.connections_of.borrow().as_deref() != Some(document.as_str()) {
            *self.connections_of.borrow_mut() = Some(document.clone());
            self.connections_panel.forget_more();
            self.connections_panel.show(Connections { outgoing, ..Connections::default() });
        } else {
            self.connections_panel.set_outgoing(outgoing);
        }
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        *self.connections_job.borrow_mut() = std::sync::Arc::clone(&cancelled);
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let name = document_name(&document).to_owned();
        std::thread::spawn(move || {
            let found = crate::connections_worker::gather(&vault, &index_dir, &document, &name, &links, &cancelled);
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                // A newer list asked for, or another document: this one is stale.
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let (Some(window), Some((linked, unlinked))) = (target.get(mtm).upgrade(), found)
                    && window.connections_of.borrow().as_deref() == Some(document.as_str())
                {
                    window.connections_panel.set_mentions(linked, unlinked);
                }
            });
        });
    }

    /// The document's own links (a note's, as typed): only they change as it is edited.
    fn refresh_outgoing(&self) {
        if self.panel_mode.get() != PanelMode::Links {
            return;
        }
        if let Some(document) = self.connections_of.borrow().clone() {
            self.connections_panel.set_outgoing(self.outgoing_of(&document));
        }
    }

    /// The links of `document` when it is the note open, as the editor holds it.
    fn outgoing_of(&self, document: &str) -> Vec<knotes_app::Outgoing> {
        let Some(links) = self.state.borrow().links.clone() else { return Vec::new() };
        match self.open_path() {
            Some(note) if note == document => {
                let text = self.note_text.string().to_string();
                knotes_app::outgoing_links(&note, &text, &MarkdownParser::new().parse(&text), &*links)
            }
            _ => Vec::new(),
        }
    }

    /// A line in the connections clicked: its note opened in the tab shown, the link at
    /// `range` (bytes) selected, in view and pointed at.
    fn open_connection(&self, path: &str, range: Option<std::ops::Range<usize>>) {
        self.open_note(path, true);
        let Some(range) = range else { return };
        let text = self.note_text.string().to_string();
        if range.end > text.len() || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            return;
        }
        let units = utf16_offsets(&text, &[range.start, range.end]);
        let selected = NSRange::new(units[0], units[1] - units[0]);
        self.note_text.setSelectedRange(selected);
        self.note_text.scrollRangeToVisible(selected);
        self.note_text.showFindIndicatorForRange(selected);
    }

    /// An outgoing link in the connections clicked: its note opened, or made when it is not
    /// there yet, as a click on the link does.
    fn follow_connection(&self, label: &str, resolved: Option<String>) {
        match resolved {
            Some(path) => self.open_note(&path, true),
            None => self.follow_link(&Link {
                kind: LinkKind::Wiki,
                embed: false,
                target: label.to_owned(),
                subpath: None,
                display: None,
                span: knotes_core::Span { start: 0, end: 0 },
            }),
        }
    }

    /// An unlinked mention's button: the mention at `range` (bytes) in the note at `source`
    /// made a link to the document open, its words kept as the link's text.
    fn make_connection_link(&self, source: &str, range: std::ops::Range<usize>) {
        let document = self.pdf_path.borrow().clone().or_else(|| self.open_path());
        let (Some(document), Some(vault), Some(links)) = (document, self.vault(), self.state.borrow().links.clone()) else { return };
        let files = FsVault::new(&vault);
        let Ok(text) = files.read_text(source) else { return };
        // The note changed since: nothing is made.
        let name = document_name(&document);
        if !text.get(range.clone()).is_some_and(|words| words.eq_ignore_ascii_case(name) || words.to_lowercase() == name.to_lowercase()) {
            return;
        }
        let new = knotes_app::make_link(&text, range, &links.link_text(&document, source));
        if let Err(e) = files.write_note(source, &new) {
            eprintln!("knotes: cannot make a link in {source}: {e}");
        }
        self.refresh_connections();
    }

    /// Lists the headings of the document open in the outline, when the panel shows it.
    fn refresh_outline(&self) {
        if self.panel_mode.get() != PanelMode::Outline {
            return;
        }
        let pdf = self.pdf_path.borrow().clone();
        let note = self.editing.borrow().as_ref().map(|e| e.path.clone());
        let (outline, empty) = if let Some(path) = &pdf {
            let bookmarks = self.vault().and_then(|v| knotes_pdf::pdf_outline(&v.join(path))).unwrap_or_default();
            let entries = bookmarks.into_iter().map(|b| OutlineEntry { level: b.level, title: b.title, place: (b.page, b.y) }).collect();
            (Outline::new(entries), strings::NO_PDF_OUTLINE)
        } else if note.is_some() {
            let text = self.note_text.string().to_string();
            let headings = note_headings(&text);
            let offsets = utf16_offsets(&text, &headings.iter().map(|h| h.span.start as usize).collect::<Vec<_>>());
            let entries = headings.into_iter().zip(offsets).map(|(h, at)| OutlineEntry { level: h.level, title: h.text, place: (at, 0.0) }).collect();
            (Outline::new(entries), strings::NO_HEADINGS)
        } else {
            (Outline::default(), "")
        };
        let document = pdf.or(note);
        let same = *self.outline_of.borrow() == document;
        *self.outline_of.borrow_mut() = document;
        self.outline_panel.show(outline, empty, same);
        self.outline_follow();
    }

    /// Scrolls the document open to heading `heading` of the outline (a click on it).
    fn go_to_heading(&self, heading: usize) {
        let Some(entry) = self.outline_panel.entry(heading) else { return };
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.go_to_place(entry.place.0, entry.place.1);
        } else if self.editing.borrow().is_some() {
            self.scroll_note_to(entry.place.0);
        }
        self.outline_jump.set(self.document_scroll().map(|(y, _)| (heading, y)));
        self.outline_follow();
    }

    /// How far the document open is scrolled, and the place at the top of its view.
    fn document_scroll(&self) -> Option<(f64, knotes_app::OutlinePlace)> {
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            return Some((viewer.scroll_y(), viewer.top_place()));
        }
        let y = self.note_text.enclosingScrollView()?.contentView().bounds().origin.y;
        Some((y, (self.note_top()?, 0.0)))
    }

    /// Highlights in the outline the heading of the part of the document in view: the one
    /// clicked last while the document stays where that put it.
    fn outline_follow(&self) {
        if self.panel_mode.get() != PanelMode::Outline {
            return;
        }
        let Some((y, top)) = self.document_scroll() else { return self.outline_panel.set_current(None) };
        let current = match self.outline_jump.get() {
            Some((heading, at)) if (at - y).abs() < 0.5 => Some(heading),
            _ => {
                self.outline_jump.set(None);
                self.outline_panel.current_at(top)
            }
        };
        self.outline_panel.set_current(current);
    }

    /// Scrolls the note so the line at `offset` (UTF-16) is at the top, under the toolbar (as
    /// far as the note goes).
    fn scroll_note_to(&self, offset: usize) {
        let view = &self.note_text;
        // Laid out around it first: TextKit only estimates where lines out of view are.
        view.scrollRangeToVisible(NSRange::new(offset, 0));
        let (Some(layout), Some(scroll)) = (view.textLayoutManager(), view.enclosingScrollView()) else { return };
        let Some(content) = layout.textContentManager() else { return };
        let Some(fragment) = content.locationFromLocation_withOffset(&content.documentRange().location(), offset as isize).and_then(|at| layout.textLayoutFragmentForLocation(&at)) else { return };
        let y = fragment.layoutFragmentFrame().origin.y + view.textContainerOrigin().y;
        let clip = scroll.contentView();
        clip.scrollToPoint(NSPoint::new(clip.bounds().origin.x, y - scroll.contentInsets().top));
        scroll.reflectScrolledClipView(&clip);
        layout.textViewportLayoutController().layoutViewport();
    }

    /// The offset (UTF-16) of the paragraph at the top of the note's visible part, under the
    /// toolbar.
    pub fn note_top(&self) -> Option<usize> {
        let view = &self.note_text;
        let (layout, scroll) = (view.textLayoutManager()?, view.enclosingScrollView()?);
        let content = layout.textContentManager()?;
        let y = scroll.contentView().bounds().origin.y + scroll.contentInsets().top - view.textContainerOrigin().y + 1.0;
        let fragment = layout.textLayoutFragmentForPosition(NSPoint::new(1.0, y.max(0.0)))?;
        Some(content.offsetFromLocation_toLocation(&content.documentRange().location(), &fragment.rangeInElement().location()).max(0) as usize)
    }

    /// An edit: the outline is listed again once typing pauses briefly (typing stays as fast).
    fn outline_later(&self) {
        if self.panel_mode.get() == PanelMode::Ai || self.outline_due.replace(true) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let when = DispatchTime::try_from(OUTLINE_DELAY).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(window) = target.get(mtm).upgrade() {
                window.outline_due.set(false);
                // Typing changes the outline and the note's own links, not who links to it.
                window.refresh_outline();
                window.refresh_outgoing();
            }
        });
    }

    /// What the right panel shows, and the view showing (probe mode).
    pub fn describe_panel(&self) -> String {
        let shows = if !self.chat.view.isHidden() {
            "chat"
        } else if !self.outline_panel.view.isHidden() {
            "outline"
        } else if !self.connections_panel.view.isHidden() {
            "links"
        } else {
            "nothing"
        };
        format!("{}, shows {shows}", self.panel_mode.get().key())
    }

    /// Called when the user flips the AI/outline switch (the app saves the choice).
    /// The right panel shown (unfolded if folded away) with the chat, its question field
    /// focused (View → Ask the AI, Cmd+L).
    pub fn ask_ai(&self) {
        if self.panel_item.isCollapsed() {
            self.panel_item.setCollapsed(false);
        }
        if self.panel_mode.get() != PanelMode::Ai {
            self.set_panel_mode(PanelMode::Ai);
        }
        self.chat.focus();
    }

    pub fn on_panel_mode(&self, callback: impl Fn(PanelMode) + 'static) {
        let weak = self.this.borrow().clone();
        self.toolbar_delegate.on_panel_mode(move |mode| {
            if let Some(window) = weak.upgrade() {
                window.show_panel(mode);
            }
            callback(mode);
        });
    }

    fn rebuild_sidebar(&self) {
        let model = {
            let state = self.state.borrow();
            if state.sidebar_mode == SidebarMode::Tabs {
                let tabs: Vec<(String, String)> = self.tabs.borrow().paths().map(|p| (p.to_owned(), document_name(p).to_owned())).collect();
                drop(state);
                self.sidebar.set_model(Sidebar::tabs(&tabs));
                self.outline.reloadData();
                return;
            }
            let inbox = new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER);
            Sidebar::new(&state.folders, &state.tags, state.sidebar_mode, Some(&inbox), state.untagged, &|path| {
                resolve_list_settings(path, &state.rules).include_subfolders
            })
        };
        self.sidebar.set_model(model);
        self.outline.reloadData();
    }

    /// Images pasted (Cmd+V) or dropped (at character `at`) into the open note: copied into
    /// the attachments folder (one already in the vault stays where it is) and embedded where
    /// the cursor is or the drop falls, each on a line of its own (ADR 0020). False when the
    /// pasteboard brings no image: the text view pastes as usual.
    pub fn paste_images(&self, pasteboard: &NSPasteboard, at: Option<usize>) -> bool {
        let Some(vault) = self.state.borrow().vault.clone() else { return false };
        let found = pasteboard_images(pasteboard, &vault);
        if found.is_empty() {
            return false;
        }
        let store = FsVault::new(&vault);
        let paths: Vec<String> = found
            .into_iter()
            .filter_map(|image| match image {
                PastedImage::InVault(path) => Some(path),
                PastedImage::New { name, bytes } => store.add_attachment(&name, &bytes).inspect_err(|e| eprintln!("knotes: cannot add {name}: {e}")).ok(),
            })
            .collect();
        if paths.is_empty() {
            return true;
        }
        let view: &objc2_app_kit::NSTextView = &self.note_text;
        let text = view.string();
        if let Some(at) = at {
            view.setSelectedRange(NSRange::new(at.min(text.length()), 0));
        }
        let selection = view.selectedRange();
        let end = selection.location + selection.length;
        let line_start = text.lineRangeForRange(NSRange::new(selection.location, 0)).location;
        let after_line = text.lineRangeForRange(NSRange::new(end, 0));
        let line_end = (after_line.location + after_line.length).max(end);
        let before = text.substringWithRange(NSRange::new(line_start, selection.location - line_start)).to_string();
        let after = text.substringWithRange(NSRange::new(end, line_end - end)).to_string();
        let catalog = Arc::clone(&self.state.borrow().catalog);
        let bare = |path: &str| {
            let name = path.rsplit('/').next().unwrap_or(path);
            !catalog.iter().any(|e| e.path != path && e.path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        };
        let embeds = embed_images(&paths, &before, after.trim_end_matches(['\n', '\r']), bare);
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&embeds), not_found) };
        true
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
                let mut rules = state.rules.get(&folder).cloned().unwrap_or_else(|| FolderRules::parse("{}").expect("empty object"));
                rules.set_option(option);
                if let Some(vault) = &state.vault
                    && let Err(e) = FsVault::new(vault).write_rules(&folder, &rules.to_json())
                {
                    eprintln!("knotes: cannot save {folder}/.knotes.json: {e}");
                }
                Arc::make_mut(&mut state.rules).insert(folder, rules);
            }
        }
        drop(state);
        self.refresh_list();
    }

    pub fn apply(&self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => self.show(&snapshot),
            Event::Error(e) => eprintln!("knotes: indexing stopped: {e}"),
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
        let changed = self.open_path().filter(|open| snapshot.changed.iter().any(|p| p == open));
        {
            let mut state = self.state.borrow_mut();
            state.rows = Arc::clone(&snapshot.rows);
            state.rules = Arc::clone(&snapshot.rules);
            state.folders = Arc::clone(&snapshot.folders);
            state.tags = Arc::clone(&snapshot.tags);
            state.untagged = snapshot.untagged;
            state.catalog = Arc::clone(&snapshot.catalog);
        }
        let new_links = {
            let mut state = self.state.borrow_mut();
            let fresh = !state.links.as_ref().is_some_and(|l| Arc::ptr_eq(l, &snapshot.links));
            state.titles = Some(Arc::clone(&snapshot.titles));
            fresh.then(|| {
                state.links = Some(Arc::clone(&snapshot.links));
                Arc::clone(&snapshot.links)
            })
        };
        if let Some(links) = new_links {
            self.live_preview.set_links(&self.note_text, links);
        } else {
            self.live_preview.files_changed(&self.note_text, &snapshot.changed);
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
        self.update_navigation();
        if probe::enabled() && !snapshot.rows.is_empty() && probe::act_once() {
            probe::sidebar_mode_requested(&self.window);
            probe::search_requested(&self.window);
            probe::toggle_sidebar_requested(&self.window);
            probe::panel_requested(&self.window);
            if let Some(item) = probe::requested_selection() {
                self.highlight(&item);
                self.select(item);
            }
            if let Some(option) = probe::requested_option() {
                self.choose(option);
            }
            if probe::new_note_requested() {
                probe::type_requested(&self.note_text);
            }
            for (i, path) in probe::requested_notes().iter().enumerate() {
                if let Some(row) = self.list.row_of(path) {
                    // Selecting the row goes through the same delegate path as a click.
                    self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false);
                    // Shown before the next one opens, as on screen.
                    self.window.displayIfNeeded();
                } else if FileKind::from_path(path) == FileKind::Pdf {
                    // A PDF is listed only among a search's results, which come later: opened
                    // as a click on its result opens it.
                    self.open_note(path, true);
                    self.window.displayIfNeeded();
                }
                if i == 0 {
                    probe::type_requested(&self.note_text);
                    probe::keys_requested(&self.note_text);
                    if let Some(row) = std::env::var("KNOTES_PROBE_COMPLETION_CLICK").ok().and_then(|r| r.parse().ok()) {
                        self.completion.click_row(row);
                    }
                }
            }
            probe::save_requested();
            probe::write_requested(self);
            probe::rewrite_image_requested(self);
            if let (Some(name), Some(open)) = (probe::rename_requested(), self.open_path()) {
                self.rename_note(&open, &name);
            }
            probe::list_menu_requested(&self.list, &self.table);
            probe::drag_requested(&self.list, &self.table, &self.sidebar);
            probe::folder_rename_requested(&self.sidebar, &self.outline);
            if let Some(forward) = probe::requested_navigation() {
                self.navigate(forward);
            }
            if let Some(width) = std::env::var("KNOTES_PROBE_WIDTH").ok().and_then(|w| w.parse::<f64>().ok()) {
                let height = self.window.frame().size.height;
                self.window.setContentSize(objc2_foundation::NSSize::new(width, height));
                self.window.layoutIfNeeded();
            }
            if let Ok(cursor) = std::env::var("KNOTES_PROBE_CURSOR") {
                let end = self.note_text.string().length();
                let at = if cursor == "end" { end } else { cursor.parse().unwrap_or(0).min(end) };
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(at, 0));
            }
            probe::paste_requested(&self.note_text);
            probe::image_edge_requested(&self.note_text);
            probe::image_hover_requested(&self.note_text);
            probe::pointer_requested(&self.note_text);
            if let Ok(scroll) = std::env::var("KNOTES_PROBE_TABLE_SCROLL") {
                probe::table_scroll_requested(&self.note_text, &scroll);
            }
            if let Ok(spec) = std::env::var("KNOTES_PROBE_TABLE_CONTEXT_AT") {
                self.live_preview.probe_table_context_at(&self.note_text, &spec);
            }
            if let Ok(spec) = std::env::var("KNOTES_PROBE_TABLE_CONTEXT") {
                self.live_preview.probe_table_context(&self.note_text, &spec);
            }
            if let Ok(press) = std::env::var("KNOTES_PROBE_TABLE_PRESS") {
                self.live_preview.probe_table_press(&self.note_text, &press);
            }
            if let Ok(ranges) = std::env::var("KNOTES_PROBE_RANGE_X") {
                probe::report_range_xs(&self.note_text, &ranges);
            }
            if let Ok(ranges) = std::env::var("KNOTES_PROBE_SPELLING") {
                let answers: Vec<&str> = ranges
                    .split(';')
                    .filter_map(|r| r.split_once(','))
                    .filter_map(|(a, b)| Some(NSRange::new(a.parse().ok()?, b.parse().ok()?)))
                    .map(|range| if self.live_preview.spelling_hidden(range) { "dropped" } else { "kept" })
                    .collect();
                println!("spelling: {}", answers.join(" "));
            }
            if let Some(line) = std::env::var("KNOTES_PROBE_QUICK_LOOK").ok().and_then(|l| l.parse().ok()) {
                // The file the panel would show; the panel itself is not opened in probe mode.
                println!("quick-look: {}", self.note_text.image_file(line).map(|f| f.display().to_string()).unwrap_or_default());
            }
            if let Some((line, width)) = probe::resize_requested() {
                self.live_preview.resize_image(&self.note_text, line, width, true);
            }
            probe::menu_requested();
            if std::env::var("KNOTES_PROBE_SHORTCUT_FOCUS").as_deref() == Ok("list") {
                probe::shortcut_requested(&self.window, &self.table);
            } else {
                probe::shortcut_requested(&self.window, &self.note_text);
            }
            if let Ok(word) = std::env::var("KNOTES_PROBE_FIND") {
                self.open_find();
                self.find_bar.set_text(&word);
                self.find_text(&word, true);
                let steps: usize = std::env::var("KNOTES_PROBE_FIND_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
                for _ in 0..steps {
                    self.find_step(true);
                }
            }
            probe::click_requested(&self.note_text);
            // The click opened a PDF: the word is found in it.
            if let (Ok(word), true) = (std::env::var("KNOTES_PROBE_FIND"), self.pdf_view.borrow().is_some()) {
                self.open_find();
                self.find_bar.set_text(&word);
                self.find_text(&word, true);
            }
            probe::mouse_drag_requested(&self.note_text);
            self.live_preview.probe_code_pointer(&self.note_text);
            if let Ok(spec) = std::env::var("KNOTES_PROBE_LINK_MENU") {
                self.live_preview.probe_link_menu(&spec);
            }
            // `KNOTES_PROBE_NEW_TAB=b.md`: that note opened in a new tab, as the list's menu does.
            if let Ok(path) = std::env::var("KNOTES_PROBE_NEW_TAB") {
                self.open_in_new_tab(&path);
            }
            // `KNOTES_PROBE_TAB_CLICK=a.md`: a click on that document's tab, after the opening.
            if let Ok(path) = std::env::var("KNOTES_PROBE_TAB_CLICK") {
                let item = SidebarItem::Tab(path);
                self.highlight(&item);
                self.select(item);
            }
            // After the click: it may open a PDF to rename.
            probe::toolbar_rename_requested(&self.window);
            probe::title_clicks_requested(&self.window);
            if let Ok(title) = std::env::var("KNOTES_PROBE_OUTLINE_CLICK") {
                self.window.displayIfNeeded();
                self.outline_panel.click(&title);
            }
            if let Ok(title) = std::env::var("KNOTES_PROBE_OUTLINE_FOLD") {
                self.outline_panel.fold(&title);
            }

            if let Some(viewer) = self.pdf_view.borrow().as_ref() {
                probe::pdf_mouse_requested(viewer);
                probe::pdf_copy_requested(viewer);
            }
            if let Some(this) = self.this.borrow().upgrade() {
                probe::pdf_width_requested(&this);
                probe::connection_link_requested(&this);
            }
            if let Some(page) = std::env::var("KNOTES_PROBE_PDF_GO").ok().and_then(|p| p.parse().ok()) {
                self.window.displayIfNeeded();
                self.pdf_go_to(page);
            }
            if std::env::var("KNOTES_PROBE_AFTER_CLICK").as_deref() == Ok("back") {
                // Shown first, as on screen.
                self.window.displayIfNeeded();
                self.navigate(false);
            }
            println!("found-in-note: {}", self.live_preview.found_count());
            if std::env::var("KNOTES_PROBE_APPEARANCE").as_deref() == Ok("dark") {
                // SAFETY: AppKit's appearance name constant lives for the whole process.
                objc2_app_kit::NSAppearanceCustomization::setAppearance(&*self.window, objc2_app_kit::NSAppearance::appearanceNamed(unsafe { objc2_app_kit::NSAppearanceNameDarkAqua }).as_deref());
            }
            if let Some(at) = std::env::var("KNOTES_PROBE_HOVER").ok().and_then(|a| a.parse().ok()) {
                println!("hover: {}", if self.live_preview.clickable(at) { "hand" } else { "text" });
            }
            probe::note_scroll_requested(&self.note_text);
            probe::chat_requested(&self.chat);
            probe::schedule(true);
        }
    }

    /// Closes `path`'s tab: the document shown stays, or its neighbour's shows, or nothing.
    pub fn close_tab(&self, path: &str) {
        let Some(i) = self.tabs.borrow().paths().position(|p| p == path) else { return };
        let after = self.tabs.borrow_mut().close(i);
        match after {
            AfterClose::Show(next) => self.open_note(&next, true),
            AfterClose::Nothing => self.show_nothing(),
            AfterClose::Same => {}
        }
        self.save_tabs();
        if self.state.borrow().sidebar_mode == SidebarMode::Tabs {
            self.rebuild_sidebar();
            self.highlight(&SidebarItem::Gap);
        }
    }

    /// Closes the tab shown (Cmd+W); false when there is none.
    pub fn close_current_tab(&self) -> bool {
        let Some(path) = self.tabs.borrow().current_path().map(str::to_owned) else { return false };
        self.close_tab(&path);
        true
    }

    /// Closes every tab but `path`'s, which shows.
    pub fn close_other_tabs(&self, path: &str) {
        let Some(i) = self.tabs.borrow().paths().position(|p| p == path) else { return };
        let after = self.tabs.borrow_mut().close_others(i);
        if let AfterClose::Show(next) = after {
            self.open_note(&next, true);
        }
        self.save_tabs();
        if self.state.borrow().sidebar_mode == SidebarMode::Tabs {
            self.rebuild_sidebar();
            self.highlight(&SidebarItem::Gap);
        }
    }

    /// The note column as it should be: the note, the PDF (its viewer covers it), or, with no
    /// document open, a word saying so and no text view to type into (typed text went nowhere).
    fn show_note_column(&self) {
        let empty = self.editing.borrow().is_none() && self.pdf_path.borrow().is_none();
        self.no_document.setHidden(!empty);
        self.note_text.setEditable(!empty);
        if let Some(scroll) = self.note_text.enclosingScrollView() {
            scroll.setHidden(empty || self.pdf_path.borrow().is_some());
        }
    }

    /// What the PDF viewer highlights as found, and whether the find bar is over its pages
    /// (probe mode).
    pub fn describe_pdf_find(&self) -> Option<(String, bool)> {
        let viewer = self.pdf_view.borrow();
        let viewer = viewer.as_ref()?;
        Some((viewer.describe_found(), self.find_bar.is_over(&viewer.scroll)))
    }

    /// `empty: …`, `note (editable)` or `pdf`: what the note column shows (probe mode).
    pub fn describe_note_column(&self) -> String {
        if !self.no_document.isHidden() {
            return format!("empty: {}", self.no_document.stringValue());
        }
        if self.pdf_path.borrow().is_some() {
            return "pdf".to_owned();
        }
        format!("note ({})", if self.note_text.isEditable() { "editable" } else { "read only" })
    }

    /// The note column empty: no document open.
    fn show_nothing(&self) {
        self.leave();
        self.completion.hide();
        if self.find_bar.is_shown() {
            self.close_find();
        }
        *self.editing.borrow_mut() = None;
        self.show_pdf(None);
        self.live_preview.show(&self.note_text, "", "");
        self.show_note_column();
        self.refresh_panel();
        self.update_navigation();
    }

    /// The document at `path` shown as a tab (ADR 0024): the sidebar's tabs follow. Gives
    /// where the document was when its tab was last left.
    fn show_tab(&self, path: &str) -> Option<TabPlace> {
        // A note opens in the tab shown; a PDF, or a note asked to, in a new tab beside it
        // (owner's choice).
        let beside = self.new_tab_next.replace(false) || FileKind::from_path(path) == FileKind::Pdf;
        let place = if beside { self.tabs.borrow_mut().open_beside(path) } else { self.tabs.borrow_mut().navigate(path) };
        if self.state.borrow().sidebar_mode == SidebarMode::Tabs {
            self.rebuild_sidebar();
            self.highlight(&SidebarItem::Gap);
        }
        place
    }

    /// The tab shown remembers where its document is, as it is left.
    fn remember_place(&self) {
        // Only the current tab's document: a tab just closed gives its place to none.
        let shown = self.pdf_path.borrow().clone().or_else(|| self.open_path());
        if shown.is_none() || shown.as_deref() != self.tabs.borrow().current_path() {
            return;
        }
        let place = if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            Some(TabPlace { top: viewer.place(), cursor: None })
        } else if self.editing.borrow().is_some() {
            let selection = self.note_text.selectedRange();
            self.note_top().map(|top| TabPlace { top: (top, 0.0), cursor: Some((selection.location, selection.length)) })
        } else {
            None
        };
        if let Some(place) = place {
            self.tabs.borrow_mut().remember(place);
        }
    }

    /// The vault's tabs file, beside its index (ADR 0024).
    fn tabs_file(&self) -> Option<PathBuf> {
        self.state.borrow().index_dir.as_ref().map(|dir| dir.join(TABS_FILE))
    }

    /// Writes the tabs and their places for the next launch.
    fn save_tabs(&self) {
        let Some(file) = self.tabs_file() else { return };
        let text = self.tabs.borrow().to_json().to_string();
        let tmp = file.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &file)) {
            eprintln!("knotes: cannot save tabs: {e}");
        }
    }

    /// The tabs of the last launch, those whose documents are still there; the one shown
    /// then is shown again, where it was. Once, on the vault's first snapshot.
    fn restore_tabs(&self) {
        let (Some(file), Some(vault)) = (self.tabs_file(), self.vault()) else { return };
        let Ok(text) = std::fs::read_to_string(file) else { return };
        let mut tabs = Tabs::from_text(&text);
        tabs.retain(|p| vault.join(p).is_file());
        let current = tabs.current_path().map(str::to_owned);
        *self.tabs.borrow_mut() = tabs;
        if let Some(path) = current {
            self.open_note(&path, true);
        }
    }

    /// Opens `path` in a new tab beside the tab shown (the list's menu).
    pub fn open_in_new_tab(&self, path: &str) {
        self.new_tab_next.set(true);
        self.open_note(path, true);
        self.new_tab_next.set(false);
    }

    /// `other`'s tab clicked, then the tab shown before (probe mode).
    pub fn tab_trip(&self, other: &str) {
        let Some(back) = self.tabs.borrow().current_path().map(str::to_owned) else { return };
        self.select(SidebarItem::Tab(other.to_owned()));
        self.select(SidebarItem::Tab(back));
    }

    /// Shows a note in the note column; `record` adds it to the back/forward history.
    pub fn open_note(&self, path: &str, record: bool) {
        if FileKind::from_path(path) == FileKind::Pdf {
            self.open_pdf(path, None, record);
            return;
        }
        self.remember_place();
        self.show_pdf(None);
        self.leave();
        self.completion.hide();
        let text = {
            let mut state = self.state.borrow_mut();
            if record {
                state.history.visit(path);
            }
            // The file, not the index: the index may not have caught up with the last save yet.
            state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(path).ok()).unwrap_or_default()
        };
        *self.editing.borrow_mut() = Some(Editing { path: path.to_owned(), document: Document::open(&text) });
        self.show_note_column();
        self.live_preview.show(&self.note_text, path, &text);
        // Where it was when its tab was left; words found by a search, below, win.
        if let Some(place) = self.show_tab(path) {
            self.scroll_note_to(place.top.0);
            if let Some((at, length)) = place.cursor.filter(|(at, length)| at + length <= self.note_text.string().length()) {
                self.note_text.setSelectedRange(NSRange::new(at, length));
            }
        }
        self.show_found_words(&text);
        if self.find_bar.is_shown() {
            self.find_text(&self.find_bar.text(), true);
        }
        self.find_search_word();
        self.refresh_panel();
        self.update_navigation();
    }

    /// Opens the PDF at `path` in place of the note (ADR 0022), at page `page` (from 1).
    pub fn open_pdf(&self, path: &str, page: Option<u32>, record: bool) {
        self.remember_place();
        self.leave();
        self.completion.hide();
        if self.find_bar.is_shown() {
            self.close_find();
        }
        if record {
            self.state.borrow_mut().history.visit(path);
        }
        // Opened from the note (its view now hidden): the PDF takes the keys; from the list, the
        // focus stays there.
        let from_note = self.from_note.replace(false) || self.window.firstResponder().is_some_and(|r| std::ptr::eq(&*r as *const _ as *const u8, &*self.note_text as *const _ as *const u8));
        *self.editing.borrow_mut() = None;
        self.show_pdf(Some(path));
        self.show_note_column();
        if from_note && let Some(viewer) = self.pdf_view.borrow().as_ref() {
            self.window.makeFirstResponder(Some(viewer.pages_view()));
        }
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            // In its place in the window first, so the jump lands where it stays.
            self.window.layoutIfNeeded();
            viewer.go_to(page.unwrap_or(1).max(1) as usize - 1);
        }
        // Where it was when its tab was left, unless a link names a page.
        let place = self.show_tab(path);
        if let (None, Some(place), Some(viewer)) = (page, place, self.pdf_view.borrow().as_ref()) {
            viewer.show_place(place.top);
        }
        // Opened from a content search: Cmd+F's bar with its word, at its first place (once
        // found, on another thread); a link naming a page keeps it.
        if page.is_none() {
            self.find_search_word();
        }
        self.refresh_panel();
        self.update_navigation();
    }

    /// Shows the PDF at `path` in the note column, or the note again (`None`). The last PDF's
    /// viewer goes, with every page it drew.
    fn show_pdf(&self, path: Option<&str>) {
        self.pdf_view.borrow_mut().take();
        let mtm = objc2::MainThreadOnly::mtm(&*self.window);
        let viewer = path.and_then(|p| PdfViewer::new(mtm, &self.vault()?.join(p), self.find_bar.column.bounds()));
        let shown = viewer.is_some();
        if let Some(viewer) = viewer {
            let weak = self.this.borrow().clone();
            viewer.on_scroll(move || {
                if let Some(window) = weak.upgrade() {
                    window.outline_follow();
                }
            });
            // Under the find bar: Cmd+F finds in the PDF too.
            self.find_bar.add_under(&viewer.scroll);
            *self.pdf_view.borrow_mut() = Some(viewer);
        }
        if let Some(scroll) = self.note_text.enclosingScrollView() {
            scroll.setHidden(shown);
        }
        *self.pdf_path.borrow_mut() = path.filter(|_| shown).map(str::to_owned);
    }

    /// `doc.pdf 2/3` for the PDF shown and its page, or `hidden` (probe mode).
    pub fn describe_pdf(&self) -> String {
        let (Some(path), Some(viewer)) = (self.pdf_path.borrow().clone(), self.pdf_view.borrow().as_ref().map(|v| v.page())) else { return "hidden".to_owned() };
        format!("{path} {}/{}", viewer.0, viewer.1)
    }

    /// The text selected in the PDF shown (probe mode).
    pub fn pdf_selection(&self) -> String {
        self.pdf_view.borrow().as_ref().map(PdfViewer::selected_text).unwrap_or_default()
    }

    /// The pages whose text the PDF viewer keeps, from 1 (probe mode).
    pub fn pdf_texts_kept(&self) -> Vec<usize> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::texts_kept).unwrap_or_default()
    }

    /// The pages the PDF viewer keeps drawn, from 1 (probe mode).
    pub fn pdf_pages_kept(&self) -> Vec<usize> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::kept).unwrap_or_default()
    }

    /// Shows page `page` (from 1) of the PDF shown (probe mode).
    pub fn pdf_go_to(&self, page: usize) {
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.go_to(page.max(1) - 1);
        }
    }

    /// Opened from a search of the notes' text: the words it found, highlighted, the first in
    /// view.
    fn show_found_words(&self, text: &str) {
        let query = self.state.borrow().search.clone();
        let (mode, words) = parse_search(&query);
        if words.is_empty() || mode == SearchMode::Files {
            return;
        }
        let found = knotes_app::found_words(text, words);
        let bounds: Vec<usize> = found.iter().flat_map(|r| [r.start, r.end]).collect();
        let units = utf16_offsets(text, &bounds);
        let ranges: Vec<(usize, usize)> = units.chunks(2).map(|p| (p[0], p[1])).collect();
        self.live_preview.show_found(&self.note_text, &ranges);
    }

    /// Follows a clicked link: opens the note it resolves to (creating a missing one), then
    /// moves to its `#heading` or `^block`; external links open in their app (ADR 0013).
    fn follow_link(&self, link: &Link) {
        if link.kind == LinkKind::External {
            open_url(&link.target);
            return;
        }
        let Some(source) = self.open_path() else { return };
        let Some(links) = self.state.borrow().links.clone() else { return };
        let path = match links.resolve(link.kind, &link.target, &source) {
            Some(path) => path,
            None => match self.create_linked_note(link, &source) {
                Some(path) => path,
                None => return,
            },
        };
        if FileKind::from_path(&path) == FileKind::Pdf {
            let page = link.subpath.as_deref().and_then(|s| s.strip_prefix("page=")).and_then(|p| p.parse().ok());
            // From the note: the PDF takes the keys.
            self.from_note.set(true);
            self.open_pdf(&path, page, true);
            self.from_note.set(false);
            return;
        }
        if path != source {
            self.open_note(&path, true);
            self.highlight_open_note();
        }
        if let Some(subpath) = &link.subpath {
            let text = self.note_text.string().to_string();
            if let Some(at) = subpath_offset(&text, subpath) {
                self.note_text.move_to_top(utf16_offsets(&text, &[at])[0]);
            }
        }
    }

    /// A link in a chat answer (`Projects/Radar`, `budget.pdf#page=3`): the note or PDF it
    /// resolves to opened, at its heading or page. A link to nothing opens nothing (unlike a
    /// note's links, which make the note).
    fn open_chat_link(&self, target: &str) {
        let (path, subpath) = match target.split_once('#') {
            Some((path, subpath)) => (path, Some(subpath.to_owned())),
            None => (target, None),
        };
        let Some(links) = self.state.borrow().links.clone() else { return };
        let source = self.open_path().unwrap_or_default();
        let Some(resolved) = links.resolve(LinkKind::Wiki, path, &source) else { return };
        if FileKind::from_path(&resolved) == FileKind::Pdf {
            let page = subpath.as_deref().and_then(|s| s.strip_prefix("page=")).and_then(|p| p.parse().ok());
            self.open_pdf(&resolved, page, true);
            return;
        }
        self.open_note(&resolved, true);
        self.highlight_open_note();
        if let Some(subpath) = &subpath {
            let text = self.note_text.string().to_string();
            if let Some(at) = subpath_offset(&text, subpath) {
                self.note_text.move_to_top(utf16_offsets(&text, &[at])[0]);
            }
        }
    }

    /// Creates the empty note a link to a missing note names: at the written path for links
    /// with a folder (Markdown links: from the linking note's folder), else in the new notes
    /// folder. An existing file (not indexed yet) is opened as it is.
    fn create_linked_note(&self, link: &Link, source: &str) -> Option<String> {
        let target = link.target.trim().trim_start_matches('/');
        if target.is_empty() {
            return None;
        }
        let state = self.state.borrow();
        let vault = state.vault.clone()?;
        let folder = match link.kind {
            LinkKind::Markdown => source.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_owned()),
            _ if target.contains('/') => String::new(),
            _ => new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
        };
        drop(state);
        let mut path = normalize(&if folder.is_empty() { target.to_owned() } else { format!("{folder}/{target}") })?;
        if !path.to_lowercase().ends_with(".md") {
            path.push_str(".md");
        }
        let fs = FsVault::new(&vault);
        if !vault.join(&path).exists()
            && let Err(e) = fs.write_note(&path, "")
        {
            eprintln!("knotes: cannot create {path}: {e}");
            return None;
        }
        Some(path)
    }

    /// Shows, updates or hides the link completion list for the text before the cursor.
    fn update_completion(&self) {
        if self.accepting.get() {
            return;
        }
        let view = &self.note_text;
        let selection = view.selectedRange();
        let ns = view.string();
        let context = (selection.length == 0)
            .then(|| {
                let line = ns.paragraphRangeForRange(NSRange::new(selection.location, 0));
                let before = ns.substringWithRange(NSRange::new(line.location, selection.location - line.location)).to_string();
                link_completion(&before).map(|c| (line.location, before, c))
            })
            .flatten();
        let Some((line, before, completion)) = context else {
            self.completion.hide();
            self.completion_dismissed.set(None);
            return;
        };
        let units = |bytes: usize| line + before[..bytes].encode_utf16().count();
        let open = units(before.rfind("[[").unwrap_or(0));
        if self.completion_dismissed.get() == Some(open) {
            return;
        }
        let from = match &completion {
            LinkCompletion::Title { from, .. } | LinkCompletion::Heading { from, .. } | LinkCompletion::Block { from, .. } => units(*from),
        };
        let items = self.completion_items(&completion);
        if items.is_empty() {
            self.completion.hide();
            return;
        }
        self.completion_from.set(from);
        // SAFETY: the range lies inside the text; a null actual range is allowed.
        let cursor = unsafe { view.firstRectForCharacterRange_actualRange(NSRange::new(selection.location, 0), std::ptr::null_mut()) };
        self.completion.show(&self.window, cursor, items);
    }

    fn completion_items(&self, completion: &LinkCompletion) -> Vec<CompletionItem> {
        let state = self.state.borrow();
        let (Some(links), Some(titles), Some(source)) = (state.links.as_ref(), state.titles.as_ref(), self.open_path()) else {
            return Vec::new();
        };
        match completion {
            LinkCompletion::Title { query, .. } => {
                let paths: Vec<(String, Option<String>)> = if query.is_empty() {
                    recent_notes(&state.rows, MAX_ROWS).into_iter().map(|p| (p, None)).collect()
                } else {
                    titles.search_titles(query, MAX_ROWS).into_iter().map(|h| (h.path, h.alias)).collect()
                };
                paths
                    .into_iter()
                    .map(|(path, alias)| {
                        let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
                        let name = name.strip_suffix(".md").unwrap_or(name);
                        let link = links.link_text(&path, &source);
                        CompletionItem {
                            title: alias.clone().unwrap_or_else(|| name.to_owned()),
                            detail: folder.to_owned(),
                            insert: alias.map_or_else(|| link.clone(), |a| format!("{link}|{a}")),
                        }
                    })
                    .collect()
            }
            LinkCompletion::Heading { target, query, .. } | LinkCompletion::Block { target, query, .. } => {
                let Some(path) = links.resolve(LinkKind::Wiki, target, &source) else { return Vec::new() };
                let text = if path == source {
                    self.note_text.string().to_string()
                } else {
                    state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(&path).ok()).unwrap_or_default()
                };
                let query = query.to_lowercase();
                if matches!(completion, LinkCompletion::Block { .. }) {
                    // The block anchors of the target, each with the start of its line.
                    return MarkdownParser::new()
                        .parse(&text)
                        .anchors
                        .into_iter()
                        .filter(|a| a.name.to_lowercase().contains(&query))
                        .take(MAX_ROWS)
                        .map(|a| {
                            let start = text[..a.span.start as usize].rfind('\n').map_or(0, |n| n + 1);
                            let line: String = text[start..a.span.start as usize].trim().chars().take(60).collect();
                            CompletionItem { title: format!("^{}", a.name), detail: line, insert: format!("^{}", a.name) }
                        })
                        .collect();
                }
                note_headings(&text)
                    .into_iter()
                    .filter(|h| h.text.to_lowercase().contains(&query))
                    .take(MAX_ROWS)
                    .map(|h| CompletionItem { insert: h.text.clone(), title: h.text, detail: format!("H{}", h.level) })
                    .collect()
            }
        }
    }

    /// Keys while the list is open: Up/Down choose, Return/Tab insert, Esc closes.
    fn completion_command(&self, command: Sel) -> bool {
        if !self.completion.is_visible() {
            return false;
        }
        if command == sel!(moveDown:) {
            self.completion.move_selection(1);
        } else if command == sel!(moveUp:) {
            self.completion.move_selection(-1);
        } else if command == sel!(insertNewline:) || command == sel!(insertTab:) {
            self.accept_completion();
        } else if command == sel!(cancelOperation:) {
            self.completion.hide();
            let view = &self.note_text;
            let ns = view.string();
            let cursor = view.selectedRange().location;
            let line = ns.paragraphRangeForRange(NSRange::new(cursor, 0));
            let before = ns.substringWithRange(NSRange::new(line.location, cursor - line.location)).to_string();
            let open = before.rfind("[[").map(|b| line.location + before[..b].encode_utf16().count());
            self.completion_dismissed.set(open);
        } else {
            return false;
        }
        true
    }

    /// Replaces the typed part with the chosen link (or heading), closing it with `]]` unless
    /// the brackets already follow, and puts the cursor after them.
    fn accept_completion(&self) {
        let Some(item) = self.completion.chosen() else { return };
        self.completion.hide();
        let view = &self.note_text;
        let cursor = view.selectedRange().location;
        let from = self.completion_from.get().min(cursor);
        let ns = view.string();
        let closed = cursor + 2 <= ns.length() && ns.substringWithRange(NSRange::new(cursor, 2)).to_string() == "]]";
        let text = if closed { item.insert } else { format!("{}]]", item.insert) };
        self.accepting.set(true);
        view.setSelectedRange(NSRange::new(from, cursor - from));
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { NSTextInputClient::insertText_replacementRange(&***view, &NSString::from_str(&text), not_found) };
        let after = from + text.encode_utf16().count() + if closed { 2 } else { 0 };
        view.setSelectedRange(NSRange::new(after, 0));
        self.accepting.set(false);
    }

    /// Image `line`'s edge dragged to `width` and let go (probe mode).
    pub fn resize_image(&self, line: usize, width: f64) {
        self.live_preview.resize_image(&self.note_text, line, width, true);
    }

    /// What each of the note's images shows (probe mode).
    pub fn images_shown(&self) -> Vec<String> {
        self.live_preview.images_shown()
    }

    /// The PDF viewer made `width` wide (probe mode).
    pub fn pdf_set_width(&self, width: f64) {
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            let frame = viewer.scroll.frame();
            viewer.scroll.setFrameSize(objc2_foundation::NSSize::new(width, frame.size.height));
        }
    }

    /// The PDF viewer's view taking the keys (probe mode).
    pub fn pdf_pages_view(&self) -> Option<Retained<NSView>> {
        self.pdf_view.borrow().as_ref().map(|v| objc2::Message::retain(v.pages_view()))
    }

    /// The PDF viewer's page label and whether it shows (probe mode).
    pub fn pdf_page_label(&self) -> Option<String> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::describe_page_label)
    }

    /// How far the PDF viewer is scrolled and its magnification (probe mode).
    pub fn pdf_scroll_and_zoom(&self) -> Option<(f64, f64)> {
        self.pdf_view.borrow().as_ref().map(|v| (v.scroll_y(), v.magnification()))
    }

    /// The page at the top of the PDF viewer, from 1 (probe mode).
    pub fn pdf_top_page(&self) -> Option<usize> {
        self.pdf_view.borrow().as_ref().map(|v| v.top_place().0 + 1)
    }

    /// What each page the PDF viewer keeps shows (probe mode).
    pub fn pdf_drawn(&self) -> Vec<String> {
        self.pdf_view.borrow().as_ref().map(PdfViewer::drawn).unwrap_or_default()
    }

    /// How many of the open note's images have been decoded, and how many are kept (probe
    /// mode).
    pub fn decoded_images(&self) -> (Vec<String>, usize) {
        self.live_preview.decoded_images()
    }

    /// `light` or `dark`: the colour formulas are typeset in (probe mode).
    pub fn formula_ink(&self) -> &'static str {
        self.live_preview.formula_ink()
    }

    /// Probe lines of what the note draws in place of hidden source, in view.
    pub fn describe_drawn(&self) -> Vec<String> {
        LivePreview::describe_drawn(&self.note_text)
    }

    pub fn describe_completion(&self) -> String {
        self.completion.describe()
    }

    /// Path of the note in the editor.
    pub fn open_path(&self) -> Option<String> {
        self.editing.borrow().as_ref().map(|e| e.path.clone())
    }

    pub fn vault(&self) -> Option<PathBuf> {
        self.state.borrow().vault.clone()
    }

    /// The open note's file as it is on disk now (probe mode).
    pub fn disk_text(&self) -> Option<String> {
        let path = self.open_path()?;
        let vault = self.state.borrow().vault.clone()?;
        FsVault::new(&vault).read_text(&path).ok()
    }

    /// Whether the editor has edits not written yet.
    pub fn is_dirty(&self) -> bool {
        self.editing.borrow().as_ref().is_some_and(|e| e.document.is_dirty())
    }

    /// A user edit: autosave once the user pauses.
    fn edited(&self) {
        match self.editing.borrow_mut().as_mut() {
            Some(editing) => editing.document.edited(),
            None => return,
        }
        let edit = self.edits.get() + 1;
        self.edits.set(edit);
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        let when = DispatchTime::try_from(AUTOSAVE_DELAY).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(window) = target.get(mtm).upgrade()
                && window.edits.get() == edit
            {
                window.save();
            }
        });
    }

    /// File → New Note: an empty `Без назви.md` in the selected folder (the new notes folder
    /// for "All notes" and tags), opened with the cursor in it (ADR 0011, 0013).
    pub fn new_note(&self) {
        let (vault, folder) = {
            let state = self.state.borrow();
            let Some(vault) = state.vault.clone() else { return };
            let folder = match &state.selection {
                SidebarItem::Folder(path) => path.clone(),
                _ => new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
            };
            (vault, folder)
        };
        let path = unique_note_path(&folder, strings::UNTITLED, |p| vault.join(p).exists());
        if let Err(e) = FsVault::new(&vault).write_note(&path, "") {
            eprintln!("knotes: cannot create {path}: {e}");
            return;
        }
        self.open_note(&path, true);
        if let Some(editing) = self.editing.borrow_mut().as_mut() {
            editing.document = Document::untitled();
        }
        self.highlight_open_note();
        self.window.makeFirstResponder(Some(&self.note_text));
    }

    /// Saves and names a new note after its first line: the user leaves the note (another
    /// note opens, the window closes, the app quits).
    pub fn leave(&self) {
        self.write_edits();
        self.name_new_note(true);
        self.remember_place();
        self.save_tabs();
    }

    /// Autosave and File → Save; names a new note whose first line is finished.
    pub fn save(&self) {
        self.write_edits();
        self.name_new_note(false);
    }

    /// Renames the note at `old` to `name` in its folder (a number added when the name is
    /// taken), with the links to it (ADR 0019). False when nothing moved.
    pub fn rename_note(&self, old: &str, name: &str) -> bool {
        let (Some(stem), Some(vault)) = (note_file_stem(name), self.vault()) else { return false };
        let folder = old.rsplit_once('/').map_or("", |(dir, _)| dir);
        // A change of case only is a rename of the same file.
        let same = |p: &str| p.to_lowercase() == old.to_lowercase();
        let new = unique_note_path(folder, &stem, |p| !same(p) && vault.join(p).exists());
        new != old && self.move_notes(Moves::from([(old.to_owned(), new)]))
    }

    /// Asks for a folder and moves the note at `path` there, with the links to it (ADR 0019).
    pub fn move_note_to_chosen_folder(&self, path: &str) {
        let folders = {
            let state = self.state.borrow();
            let mut list = Vec::new();
            let mut stack = vec![&*state.folders];
            while let Some(node) = stack.pop() {
                list.push(node.path.clone());
                stack.extend(node.children.iter().rev());
            }
            list
        };
        let name = knotes_app::title_of(path).to_owned();
        let Some(folder) = rename_dialog::ask_folder(objc2::MainThreadOnly::mtm(&*self.window), &name, &folders) else { return };
        self.move_note_to(path, &folder);
    }

    /// Moves the note at `path` into `folder` (`""`: the vault's root), with the links to it.
    pub fn move_note_to(&self, path: &str, folder: &str) {
        let Some(vault) = self.vault() else { return };
        let file = path.rsplit('/').next().unwrap_or(path);
        let wanted = if folder.is_empty() { file.to_owned() } else { format!("{folder}/{file}") };
        if wanted == path {
            return;
        }
        let new = unique_note_path(folder, knotes_app::title_of(file), |p| vault.join(p).exists());
        self.move_notes(Moves::from([(path.to_owned(), new)]));
    }

    /// Renames the folder at `folder` to `name` (a `/` in it becomes `-`), with the links into
    /// it (ADR 0019): the folder itself moves, attachments and all. False when nothing moved.
    pub fn rename_folder(&self, folder: &str, name: &str) -> bool {
        let name = name.trim().replace('/', "-");
        let Some(vault) = self.vault().filter(|_| !name.is_empty()) else { return false };
        let new = match folder.rsplit_once('/') {
            Some((parent, _)) => format!("{parent}/{name}"),
            None => name,
        };
        if new == folder || (new.to_lowercase() != folder.to_lowercase() && vault.join(&new).exists()) {
            return false;
        }
        let prefix = format!("{folder}/");
        let moves: Moves = self
            .state
            .borrow()
            .catalog
            .iter()
            .filter_map(|e| e.path.strip_prefix(&prefix).map(|rest| (e.path.clone(), format!("{new}/{rest}"))))
            .collect();
        if !self.relocate(moves, Some((folder, &new))) {
            return false;
        }
        // The sidebar keeps showing the folder (or the folder inside it) under its new name.
        let selected = match &self.state.borrow().selection {
            SidebarItem::Folder(path) if path == folder => Some(new.clone()),
            SidebarItem::Folder(path) => path.strip_prefix(&prefix).map(|rest| format!("{new}/{rest}")),
            _ => None,
        };
        if let Some(path) = selected {
            self.state.borrow_mut().selection = SidebarItem::Folder(path);
        }
        true
    }

    /// Moves notes (new path by old), asking first when links would change and rewriting them
    /// on yes (ADR 0019). The open note is saved first and follows its file. False when
    /// nothing moved.
    pub fn move_notes(&self, moves: Moves) -> bool {
        self.relocate(moves, None)
    }

    /// `move_notes`, the files moved one by one, or all at once by moving `folder` (from, to).
    fn relocate(&self, moves: Moves, folder: Option<(&str, &str)>) -> bool {
        let Some(vault_dir) = self.vault() else { return false };
        self.write_edits();
        let plan = self.relinks(&moves);
        let update = plan.links == 0
            || match rename_dialog::ask_link_update(objc2::MainThreadOnly::mtm(&*self.window), plan.links, plan.edits.len()) {
                LinkUpdate::Update => true,
                LinkUpdate::Keep => false,
                LinkUpdate::Cancel => return false,
            };
        let vault = FsVault::new(&vault_dir);
        let mut moved = Moves::new();
        if let Some((from, to)) = folder {
            match vault.move_note(from, to) {
                Ok(()) => moved = moves.clone(),
                Err(e) => {
                    eprintln!("knotes: cannot move {from} to {to}: {e}");
                    return false;
                }
            }
        }
        for (from, to) in moves.iter().filter(|_| folder.is_none()) {
            match vault.move_note(from, to) {
                Ok(()) => {
                    moved.insert(from.clone(), to.clone());
                }
                Err(e) => eprintln!("knotes: cannot move {from} to {to}: {e}"),
            }
        }
        if moved.is_empty() && folder.is_none() {
            return false;
        }
        let failed = |path: &str| moves.iter().any(|(from, to)| to == path && !moved.contains_key(from));
        let edits: Vec<&(String, String)> = if update { plan.edits.iter().filter(|(path, _)| !failed(path)).collect() } else { Vec::new() };
        for (path, text) in &edits {
            if let Err(e) = vault.write_note(path, text) {
                eprintln!("knotes: cannot update links in {path}: {e}");
            }
        }
        for (from, to) in &moved {
            self.state.borrow_mut().history.rename(from, to);
            self.tabs.borrow_mut().rename(from, to);
        }
        self.save_tabs();
        if let Some(open) = self.open_path() {
            let path = moved.get(&open).cloned().unwrap_or(open);
            let edited = edits.iter().find(|(p, _)| *p == path).map(|(_, text)| text.clone());
            if let Some(editing) = self.editing.borrow_mut().as_mut() {
                editing.path = path.clone();
                if let Some(text) = &edited {
                    editing.document.saved(text);
                }
            }
            self.live_preview.set_path(&path);
            if let Some(text) = edited {
                let cursor = self.note_text.selectedRange();
                self.live_preview.show(&self.note_text, &path, &text);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
            }
        }
        // The PDF open moved: shown from its new place, at the page it was at.
        let pdf = self.pdf_path.borrow().clone();
        if let Some(new) = pdf.and_then(|p| moved.get(&p).cloned()) {
            let page = self.pdf_view.borrow().as_ref().map_or(1, |v| v.page().0);
            self.show_pdf(Some(&new));
            if let Some(viewer) = self.pdf_view.borrow().as_ref() {
                self.window.layoutIfNeeded();
                viewer.go_to(page.saturating_sub(1));
            }
            self.refresh_panel();
        }
        self.update_navigation();
        true
    }

    /// Renames the PDF at `old` to `name` (`.pdf` added when left out) in its folder, links to
    /// it updated as a note's are; false when declined or impossible.
    pub fn rename_pdf(&self, old: &str, name: &str) -> bool {
        let name = name.trim();
        let stem = if name.to_lowercase().ends_with(".pdf") { &name[..name.len() - 4] } else { name };
        let (Some(stem), Some(vault)) = (note_file_stem(stem), self.vault()) else { return false };
        let folder = old.rsplit_once('/').map_or(String::new(), |(dir, _)| format!("{dir}/"));
        // A change of case only is a rename of the same file.
        let free = |p: &str| p.to_lowercase() == old.to_lowercase() || !vault.join(p).exists();
        let new = (0..)
            .map(|n| if n == 0 { format!("{folder}{stem}.pdf") } else { format!("{folder}{stem} {n}.pdf") })
            .find(|p| free(p))
            .unwrap_or_default();
        new != old && self.move_notes(Moves::from([(old.to_owned(), new)]))
    }

    /// The link edits `moves` call for, worked out from the index and the files.
    fn relinks(&self, moves: &Moves) -> Relinks {
        let mut state = self.state.borrow_mut();
        self.ensure_reader(&mut state);
        let (Some(reader), Some(before), Some(vault)) = (state.reader.as_ref(), state.links.clone(), state.vault.clone()) else {
            return Relinks::default();
        };
        let after = LinkTargets::new(state.catalog.iter().map(|e| moves.get(&e.path).map_or(e.path.as_str(), String::as_str)));
        let files = FsVault::new(&vault);
        plan_relinks(moves, reader, &|p| files.read_text(p).ok(), &MarkdownParser::new(), &*before, &after).unwrap_or_else(|e| {
            eprintln!("knotes: cannot plan link updates: {e}");
            Relinks::default()
        })
    }

    /// Renames a new note's file after its first line, once (ADR 0011).
    fn name_new_note(&self, leaving: bool) {
        let text = self.note_text.string().to_string();
        let old = match self.editing.borrow().as_ref() {
            Some(editing) if editing.document.wants_name(&text, leaving) => editing.path.clone(),
            _ => return,
        };
        let Some(stem) = note_file_stem(text.lines().next().unwrap_or("")) else { return };
        let Some(vault) = self.vault() else { return };
        let folder = old.rsplit_once('/').map_or("", |(dir, _)| dir);
        let path = unique_note_path(folder, &stem, |p| p != old && vault.join(p).exists());
        if path != old
            && let Err(e) = FsVault::new(&vault).move_note(&old, &path)
        {
            eprintln!("knotes: cannot rename {old} to {path}: {e}");
            return;
        }
        if let Some(editing) = self.editing.borrow_mut().as_mut() {
            editing.path = path.clone();
            editing.document.named();
        }
        self.state.borrow_mut().history.rename(&old, &path);
        self.live_preview.set_path(&path);
    }

    /// Writes the editor's text if it has unsaved edits. A change on disk the window has not
    /// seen yet is kept next to the note as a conflict copy instead of being overwritten.
    fn write_edits(&self) {
        let Some(vault) = self.state.borrow().vault.clone() else { return };
        let copy = {
            let mut editing = self.editing.borrow_mut();
            let Some(editing) = editing.as_mut().filter(|e| e.document.is_dirty()) else { return };
            let vault = FsVault::new(&vault);
            let text = self.note_text.string().to_string();
            let mut copy = None;
            if let Ok(disk) = vault.read_text(&editing.path) {
                match editing.document.on_disk(&disk, &text) {
                    DiskChange::Conflict => copy = keep_conflict_copy(&vault, &editing.path, &disk),
                    DiskChange::Ignore if !editing.document.is_dirty() => return,
                    _ => {}
                }
            }
            write(&vault, editing, &text);
            copy.map(|copy| (editing.path.clone(), copy))
        };
        if let Some((path, copy)) = copy {
            self.tell_conflict(&path, &copy);
        }
    }

    /// Tells where the other version of a note went (ADR 0011), in a sheet on the window that
    /// can open it.
    fn tell_conflict(&self, path: &str, copy: &str) {
        if probe::enabled() {
            println!("conflict-notice: {copy}");
            return;
        }
        let alert = NSAlert::new(objc2::MainThreadOnly::mtm(&*self.window));
        alert.setMessageText(&NSString::from_str(strings::CONFLICT_TITLE));
        alert.setInformativeText(&NSString::from_str(&strings::conflict_details(knotes_app::title_of(path), knotes_app::title_of(copy))));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OK));
        alert.addButtonWithTitle(&NSString::from_str(strings::CONFLICT_OPEN_COPY));
        let weak = self.this.borrow().clone();
        let copy = copy.to_owned();
        let handler = block2::RcBlock::new(move |response: NSModalResponse| {
            if response == NSAlertSecondButtonReturn
                && let Some(window) = weak.upgrade()
            {
                window.open_note(&copy, true);
            }
        });
        alert.beginSheetModalForWindow_completionHandler(&self.window, Some(&handler));
    }

    /// The open note changed on disk: our own save is ignored, a clean editor reloads, and
    /// edits on both sides keep both versions.
    fn changed_on_disk(&self, path: &str) {
        let Some(vault) = self.state.borrow().vault.clone() else { return };
        let Ok(disk) = FsVault::new(&vault).read_text(path) else { return };
        let text = self.note_text.string().to_string();
        let change = match self.editing.borrow_mut().as_mut() {
            Some(editing) => editing.document.on_disk(&disk, &text),
            None => return,
        };
        match change {
            DiskChange::Ignore => {}
            DiskChange::Reload => {
                let cursor = self.note_text.selectedRange();
                self.live_preview.show(&self.note_text, path, &disk);
                let end = self.note_text.string().length();
                self.note_text.setSelectedRange(objc2_foundation::NSRange::new(cursor.location.min(end), 0));
            }
            DiskChange::Conflict => {
                // The editor's text keeps the note's name; the copy holds the other side.
                let vault = FsVault::new(&vault);
                let copy = keep_conflict_copy(&vault, path, &disk);
                if let Some(editing) = self.editing.borrow_mut().as_mut() {
                    write(&vault, editing, &text);
                }
                if let Some(copy) = copy {
                    self.tell_conflict(path, &copy);
                }
            }
        }
    }

    /// Opens the current note again (editor bench).
    pub fn reopen_current(&self) {
        let current = self.state.borrow().history.current().map(str::to_owned);
        if let Some(path) = current {
            self.open_note(&path, false);
        }
    }

    pub fn live_preview_restyle(&self, source: &str) {
        self.live_preview.restyle_all(&self.note_text, source);
    }

    /// Back (`false`) or forward (`true`) in the note history; the list selection follows.
    pub fn navigate(&self, forward: bool) {
        let target = {
            let mut state = self.state.borrow_mut();
            let next = if forward { state.history.forward() } else { state.history.back() };
            next.map(str::to_owned)
        };
        if let Some(path) = target {
            self.open_note(&path, false);
            self.highlight_open_note();
        }
    }

    fn update_navigation(&self) {
        let state = self.state.borrow();
        self.toolbar_delegate.set_history(state.history.can_back(), state.history.can_forward());
        drop(state);
        // A note's name; a PDF's, without its folder or `.pdf` (owner's choice). Both are renamed
        // in place.
        match (self.open_path(), self.pdf_path.borrow().as_deref()) {
            (Some(note), _) => self.toolbar_delegate.set_note_title(knotes_app::title_of(&note), true),
            (None, Some(pdf)) => {
                let name = pdf.rsplit('/').next().unwrap_or(pdf);
                let stem = if name.to_lowercase().ends_with(".pdf") { &name[..name.len() - 4] } else { name };
                self.toolbar_delegate.set_note_title(stem, true);
            }
            (None, None) => self.toolbar_delegate.set_note_title("", false),
        }
    }

    /// The open note and history availability, for probe mode.
    pub fn describe_note(&self) -> (Option<String>, bool, bool) {
        let state = self.state.borrow();
        (state.history.current().map(str::to_owned), state.history.can_back(), state.history.can_forward())
    }

    fn ensure_reader(&self, state: &mut State) {
        if state.reader.is_none() {
            state.reader = state.index_dir.as_deref().and_then(|dir| SqliteIndex::open_reader(dir).ok());
        }
    }

    /// Shows the notes of a sidebar item.
    pub fn select(&self, item: SidebarItem) {
        if self.highlighting.get() {
            return;
        }
        // A tab: its document shown; the list keeps what it shows.
        if let SidebarItem::Tab(path) = &item {
            self.open_note(path, true);
            return;
        }
        self.state.borrow_mut().selection = item;
        *self.search_results.borrow_mut() = None;
        self.refresh_list();
        if !self.state.borrow().search.trim().is_empty() {
            self.request_search();
        }
    }

    fn refresh_list(&self) {
        let mut state = self.state.borrow_mut();
        let selection = state.selection.clone();
        let (title, listed) = match &selection {
            SidebarItem::Folder(path) => (path.rsplit('/').next().unwrap_or(path).to_owned(), None),
            SidebarItem::Tag(tag) => (format!("#{tag}"), Some(self.notes_with_tag(&mut state, tag))),
            SidebarItem::Untagged => (strings::UNTAGGED.to_owned(), Some(self.untagged_notes(&mut state))),
            _ => (strings::ALL_NOTES.to_owned(), None),
        };
        let filter = match (&selection, &listed) {
            (SidebarItem::Folder(path), _) => ListFilter::Folder(path),
            (_, Some(paths)) => ListFilter::Paths(paths),
            _ => ListFilter::All,
        };
        let folder = match &selection {
            SidebarItem::Folder(path) => path.as_str(),
            _ => "",
        };
        let session_key = session_settings_key(&selection);
        let settings = match &session_key {
            Some(key) => state.tag_settings.get(key).copied().unwrap_or_else(|| resolve_list_settings("", &state.rules)),
            None => resolve_list_settings(folder, &state.rules),
        };
        self.list_menu.update(&settings, session_key.is_none());
        // Search results replace the list; it is built only without them (50k notes: ~2 ms).
        // What a search here covers: the list's notes and its PDFs (ADR 0022).
        let searching = !state.search.trim().is_empty();
        *self.search_scope.borrow_mut() = (searching && !matches!(selection, SidebarItem::AllNotes)).then(|| list_scope(&state.rows, &filter, &settings));
        let results = self.search_results.borrow();
        let items = match results.as_ref().filter(|_| searching) {
            Some(results) => search_list(&state.rows, &filter, &settings, &results.titles, &results.content),
            None => build_note_list(&state.rows, &filter, &settings, &note_list_view::now()),
        };
        drop(results);
        self.list.set_items(Arc::clone(&state.rows), items);
        drop(state);
        self.table.reloadData();
        self.highlight_open_note();
        self.window.setTitle(&NSString::from_str(&title));
        let count = self.list.note_count();
        let formatted = NSNumberFormatter::localizedStringFromNumber_numberStyle(
            &NSNumber::numberWithUnsignedLongLong(count as u64),
            NSNumberFormatterStyle::DecimalStyle,
        );
        self.window.setSubtitle(&NSString::from_str(&strings::notes_count(count, &formatted.to_string())));
    }

    /// Selects the open note's row (it moves when the note is saved), or none if it is not listed.
    fn highlight_open_note(&self) {
        self.highlighting.set(true);
        match self.open_path().and_then(|p| self.list.row_of(&p)) {
            Some(row) => self.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row), false),
            // SAFETY: a nil sender is allowed.
            None => unsafe { self.table.deselectAll(None) },
        }
        self.highlighting.set(false);
    }

    /// The files/content switch: rewrites the query's operator and searches again.
    pub fn set_search_mode(&self, mode: SearchMode) {
        let text = with_search_mode(&self.search_bar.field.stringValue().to_string(), mode);
        self.search_bar.set_text(&text);
        self.search(&text);
    }

    /// Edit → Search Notes (Cmd+Shift+F): the search row shown, its field focused.
    pub fn show_search(&self) {
        self.search_bar.show(true);
    }

    /// Edit → Find… and its neighbours from anywhere in the window.
    pub fn find_in_note(&self, sender: Option<&objc2::runtime::AnyObject>) {
        self.find_action(note_view::finder_tag(sender));
    }

    /// A find action (`NSTextFinderAction`): show the bar, the next or previous occurrence, or
    /// hide the bar.
    pub fn find_action(&self, action: isize) {
        use objc2_app_kit::NSTextFinderAction as A;
        match A(action) {
            A::NextMatch | A::PreviousMatch if self.find_bar.is_shown() || !self.find_bar.text().is_empty() => {
                if !self.find_bar.is_shown() {
                    self.open_find();
                }
                self.find_step(A(action) == A::NextMatch);
            }
            A::HideFindInterface => self.close_find(),
            _ => self.open_find(),
        }
    }

    /// Shows the find bar, its text selected and found again in the note.
    fn open_find(&self) {
        self.find_bar.show(true);
        let text = self.find_bar.text();
        if !text.is_empty() {
            self.find_text(&text, true);
        }
    }

    /// Done or Escape: the bar and its highlights go, the note gets the focus back.
    pub fn close_find(&self) {
        self.find_bar.show(false);
        self.pdf_find.borrow().store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.set_found(Vec::new(), None);
        }
        self.live_preview.clear_found(&self.note_text);
        self.find_matches.borrow_mut().clear();
        self.find_current.set(None);
        self.window.makeFirstResponder(Some(&self.note_text));
    }

    /// Finds `query` in the note: every occurrence highlighted; with `jump`, the first one from
    /// the cursor on selected and in view (an edit refinds in place).
    fn find_text(&self, query: &str, jump: bool) {
        if self.pdf_view.borrow().is_some() {
            self.find_in_pdf(query, jump);
            return;
        }
        let text = self.note_text.string().to_string();
        let found = knotes_app::find_all(&text, query);
        let bounds: Vec<usize> = found.iter().flat_map(|r| [r.start, r.end]).collect();
        let units = utf16_offsets(&text, &bounds);
        let matches: Vec<(usize, usize)> = units.chunks(2).map(|p| (p[0], p[1])).collect();
        self.live_preview.highlight(&self.note_text, &matches);
        let cursor = self.note_text.selectedRange().location;
        let current = (!matches.is_empty()).then(|| matches.iter().position(|m| m.0 >= cursor).unwrap_or(0));
        *self.find_matches.borrow_mut() = matches;
        self.find_current.set(current);
        if jump && let Some(i) = current {
            self.select_match(i);
        }
        self.find_bar.show_count(self.find_current.get(), self.find_matches.borrow().len(), !query.is_empty());
    }

    /// Finds `query` in the PDF shown, its pages read one after another on another thread;
    /// with `jump`, the first place from the page shown on comes into view.
    fn find_in_pdf(&self, query: &str, jump: bool) {
        if query.is_empty() {
            self.search_pdf(Vec::new(), jump);
            self.find_bar.show_count(None, 0, false);
            return;
        }
        self.find_bar.show_searching();
        self.search_pdf(vec![query.to_owned()], jump);
    }

    /// A document opened from a content search: Cmd+F's bar shows with the search's first
    /// word, every place found highlighted and the first in view (owner's request); the focus
    /// stays where it was.
    fn find_search_word(&self) {
        let query = self.state.borrow().search.clone();
        let (mode, words) = parse_search(&query);
        let Some(word) = words.split_whitespace().next().filter(|_| mode != SearchMode::Files).map(str::to_owned) else { return };
        self.find_bar.show_with(&word);
        self.find_text(&word, true);
    }

    /// Finds `words` in the PDF shown, its pages read one after another on another thread;
    /// with `jump`, the first place from the page shown on comes into view; the find bar counts
    /// them.
    fn search_pdf(&self, words: Vec<String>, jump: bool) {
        self.pdf_find.borrow().store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(viewer) = self.pdf_view.borrow().as_ref() {
            viewer.set_found(Vec::new(), None);
        }
        self.find_current.set(None);
        let (Some(path), Some(vault), Some(mtm)) = (self.pdf_path.borrow().clone(), self.vault(), MainThreadMarker::new()) else { return };
        if words.is_empty() {
            return;
        }
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        *self.pdf_find.borrow_mut() = std::sync::Arc::clone(&cancelled);
        let target = MainThreadBound::new(self.this.borrow().clone(), mtm);
        std::thread::spawn(move || {
            let file = vault.join(&path);
            let found = words.iter().try_fold(Vec::new(), |mut all, word| {
                all.extend(knotes_pdf::find_in_pdf(&file, word, &cancelled)?);
                Some(all)
            });
            let found = found.map(|mut all| {
                all.sort_by_key(|(page, range)| (*page, range.start));
                all.dedup_by(|b, a| a.0 == b.0 && b.1.start < a.1.end);
                all
            });
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                // A newer search, or the bar closed: this one is stale.
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let (Some(window), Some(found)) = (target.get(mtm).upgrade(), found) {
                    window.pdf_found(&path, found, jump);
                }
            });
        });
    }

    /// The places a search found in the PDF at `path`, shown if it is still the one open: the
    /// first from the page shown on is the current one.
    fn pdf_found(&self, path: &str, found: Vec<(usize, std::ops::Range<usize>)>, jump: bool) {
        if self.pdf_path.borrow().as_deref() != Some(path) {
            return;
        }
        let total = found.len();
        let current = {
            let viewer = self.pdf_view.borrow();
            let Some(viewer) = viewer.as_ref() else { return };
            let page = viewer.page().0.saturating_sub(1);
            let current = found.iter().position(|(p, _)| *p >= page).or((total > 0).then_some(0));
            viewer.set_found(found, current);
            current
        };
        self.find_current.set(current);
        if jump && let (Some(i), Some(viewer)) = (current, self.pdf_view.borrow().as_ref()) {
            viewer.show_found(i);
        }
        self.find_bar.show_count(current, total, true);
    }

    /// The next (or previous) occurrence, round the note.
    fn find_step(&self, forward: bool) {
        if let Some(n) = self.pdf_view.borrow().as_ref().map(PdfViewer::found_len) {
            if n == 0 {
                return;
            }
            let i = match self.find_current.get() {
                Some(i) if forward => (i + 1) % n,
                Some(i) => (i + n - 1) % n,
                None => 0,
            };
            self.find_current.set(Some(i));
            if let Some(viewer) = self.pdf_view.borrow().as_ref() {
                viewer.show_found(i);
            }
            self.find_bar.show_count(Some(i), n, true);
            return;
        }
        let n = self.find_matches.borrow().len();
        if n == 0 {
            return;
        }
        let i = match self.find_current.get() {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None => 0,
        };
        self.find_current.set(Some(i));
        self.select_match(i);
        self.find_bar.show_count(Some(i), n, true);
    }

    /// Selects occurrence `i`, scrolls it into view and points at it.
    fn select_match(&self, i: usize) {
        let Some(&(from, to)) = self.find_matches.borrow().get(i) else { return };
        let range = objc2_foundation::NSRange::new(from, to - from);
        self.note_text.setSelectedRange(range);
        self.note_text.scrollRangeToVisible(range);
        self.note_text.showFindIndicatorForRange(range);
    }

    /// The magnifying glass: shows the search row, or hides it and drops the search.
    pub fn toggle_search(&self) {
        let shown = !self.search_bar.is_shown();
        self.search_bar.show(shown);
        if !shown {
            self.search("");
        }
    }

    /// The search field changed: the list shows the results within the selected list.
    pub fn search(&self, query: &str) {
        let (mode, text) = parse_search(query);
        self.search_bar.show_mode(mode);
        // An operator alone (`content:`) searches nothing yet: the list stays.
        self.state.borrow_mut().search = if text.is_empty() { String::new() } else { query.to_owned() };
        if text.is_empty() {
            *self.search_results.borrow_mut() = None;
            self.refresh_list();
        } else {
            *self.search_results.borrow_mut() = None;
            self.refresh_list();
            self.request_search();
        }
    }

    /// Asks the search thread for the results of the current query within the selected list;
    /// they replace the list when they arrive (`apply_search`).
    fn request_search(&self) {
        let generation = self.search_generation.get() + 1;
        self.search_generation.set(generation);
        let (query, scope, titles, index_dir) = {
            let state = self.state.borrow();
            let scope = self.search_scope.borrow().clone();
            (state.search.clone(), scope, state.titles.clone(), state.index_dir.clone())
        };
        let (mode, query) = {
            let (mode, text) = parse_search(&query);
            (mode, text.to_owned())
        };
        if self.search_worker.borrow().is_none() {
            let Some(index_dir) = index_dir else { return };
            let Some(mtm) = MainThreadMarker::new() else { return };
            let target = Arc::new(MainThreadBound::new(self.this.borrow().clone(), mtm));
            let worker = SearchWorker::start(index_dir, move |response| {
                let target = Arc::clone(&target);
                DispatchQueue::main().exec_async(move || {
                    let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
                    if let Some(window) = target.get(mtm).upgrade() {
                        window.apply_search(response);
                    }
                });
            });
            *self.search_worker.borrow_mut() = Some(worker);
        }
        if let Some(worker) = self.search_worker.borrow().as_ref() {
            worker.request(SearchRequest { generation, mode, query, scope, titles });
        }
    }

    /// Results from the search thread: shown unless a newer query is on its way.
    fn apply_search(&self, response: SearchResponse) {
        if response.generation == self.search_generation.get() && !self.state.borrow().search.trim().is_empty() {
            *self.search_results.borrow_mut() = Some(response);
            self.refresh_list();
        }
    }

    fn untagged_notes(&self, state: &mut State) -> HashSet<String> {
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.untagged_notes().ok()).unwrap_or_default().into_iter().collect()
    }

    fn notes_with_tag(&self, state: &mut State, tag: &str) -> HashSet<String> {
        self.ensure_reader(state);
        state.reader.as_ref().and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect()
    }

    fn exists(&self, item: &SidebarItem) -> bool {
        let state = self.state.borrow();
        match item {
            // The inbox row shows before its folder exists.
            SidebarItem::Folder(path) => folder_exists(&state.folders, path) || *path == new_notes_folder(&state.rules, strings::NEW_NOTES_FOLDER),
            SidebarItem::Tag(tag) => tag_exists(&state.tags, tag),
            _ => true,
        }
    }

    /// Selects the row of `item` in the sidebar without notifying (the list follows `state`).
    fn highlight(&self, item: &SidebarItem) {
        // The tabs show the document shown, whatever the list shows.
        let tab = (self.state.borrow().sidebar_mode == SidebarMode::Tabs).then(|| self.tabs.borrow().current_path().map(|p| SidebarItem::Tab(p.to_owned())));
        let item = match &tab {
            Some(Some(tab)) => tab,
            Some(None) => &SidebarItem::Gap,
            None => item,
        };
        // SAFETY: the item object comes from this outline view's data source.
        let row = unsafe { self.outline.rowForItem(Some(&self.sidebar.object_for(item))) };
        self.highlighting.set(true);
        if row >= 0 {
            self.outline.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row as usize), false);
        } else {
            // Not shown in this sidebar mode: no other row may look selected.
            // SAFETY: a nil sender is allowed.
            unsafe { self.outline.deselectAll(None) };
        }
        self.highlighting.set(false);
    }
}

/// The name a document shows: a note's without `.md`, a PDF's without `.pdf`, neither with its
/// folder.
pub(crate) fn document_name(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    [".md", ".pdf"].iter().find_map(|ext| name.len().checked_sub(ext.len()).filter(|&at| name[at..].eq_ignore_ascii_case(ext)).map(|at| &name[..at])).unwrap_or(name)
}

/// Paths of the `n` most recently modified notes, newest first.
fn recent_notes(rows: &[NoteRow], n: usize) -> Vec<String> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    let n = n.min(order.len());
    if n == 0 {
        return Vec::new();
    }
    order.select_nth_unstable_by_key(n - 1, |&i| std::cmp::Reverse(rows[i].mtime_ns));
    order.truncate(n);
    order.sort_by_key(|&i| std::cmp::Reverse(rows[i].mtime_ns));
    order.into_iter().map(|i| rows[i].path.clone()).collect()
}

/// `path` with `.` and `..` segments applied; `None` if it leaves the vault.
fn normalize(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Opens an external link in its app; probe mode only reports it.
fn open_url(url: &str) {
    if probe::enabled() {
        println!("open-url: {url}");
        return;
    }
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

fn write(vault: &FsVault, editing: &mut Editing, text: &str) {
    match vault.write_note(&editing.path, text) {
        Ok(()) => editing.document.saved(text),
        Err(e) => eprintln!("knotes: cannot save {}: {e}", editing.path),
    }
}

/// An image found on a pasteboard: a file already in the vault (its vault path), or one to
/// add to the attachments folder.
enum PastedImage {
    InVault(String),
    New { name: String, bytes: Vec<u8> },
}

/// The images a paste or a drop brings: image and PDF files (others are left), or else an image's
/// data alone; none when text comes with it (a cell copied from a table carries its picture
/// too, and the text is what is meant).
fn pasteboard_images(pasteboard: &NSPasteboard, vault: &Path) -> Vec<PastedImage> {
    // SAFETY: AppKit's pasteboard type constants live for the whole process.
    let (file_url, string, png, tiff) = unsafe { (NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardTypePNG, NSPasteboardTypeTIFF) };
    let files: Vec<PathBuf> = pasteboard
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.stringForType(file_url)?;
                    NSURL::URLWithString(&url)?.path().map(|p| PathBuf::from(p.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    if !files.is_empty() {
        let vaults = [vault.to_path_buf(), vault.canonicalize().unwrap_or_default()];
        return files
            .into_iter()
            .filter(|f| f.to_str().is_some_and(|f| matches!(FileKind::from_path(f), FileKind::Image | FileKind::Pdf)))
            .filter_map(|file| {
                let inside = vaults.iter().find_map(|v| file.strip_prefix(v).ok().and_then(Path::to_str).map(str::to_owned));
                match inside {
                    Some(path) => Some(PastedImage::InVault(path)),
                    None => Some(PastedImage::New { name: file.file_name()?.to_str()?.to_owned(), bytes: std::fs::read(&file).ok()? }),
                }
            })
            .collect();
    }
    if pasteboard.stringForType(string).is_some() {
        return Vec::new();
    }
    let bytes = pasteboard.dataForType(png).map(|d| d.to_vec()).or_else(|| {
        let data = pasteboard.dataForType(tiff)?;
        let rep = NSBitmapImageRep::imageRepWithData(&data)?;
        // SAFETY: an empty properties dictionary is valid for PNG.
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }.map(|d| d.to_vec())
    });
    let Some(bytes) = bytes else { return Vec::new() };
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyyMMddHHmmss")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    vec![PastedImage::New { name: pasted_image_name(&stamp), bytes }]
}

/// Writes the disk version of `path` as "Name (конфлікт date time).md" next to it.
fn keep_conflict_copy(vault: &FsVault, path: &str, disk: &str) -> Option<String> {
    let formatter = NSDateFormatter::new();
    formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm")));
    let stamp = formatter.stringFromDate(&NSDate::now()).to_string();
    let mut copy = conflict_path(path, &stamp);
    if vault.read_text(&copy).is_ok() {
        formatter.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH.mm.ss")));
        copy = conflict_path(path, &formatter.stringFromDate(&NSDate::now()).to_string());
    }
    match vault.write_note(&copy, disk) {
        Ok(()) => Some(copy),
        Err(e) => {
            eprintln!("knotes: cannot keep conflict copy {copy}: {e}");
            None
        }
    }
}

/// Tags and "Untagged" have no folder and no `.knotes.json`: their list options last the session.
fn session_settings_key(item: &SidebarItem) -> Option<String> {
    matches!(item, SidebarItem::Tag(_) | SidebarItem::Untagged).then(|| item.key())
}

fn folder_exists(node: &FolderNode, path: &str) -> bool {
    node.path == path || node.children.iter().any(|c| folder_exists(c, path))
}

fn tag_exists(nodes: &[TagNode], tag: &str) -> bool {
    nodes.iter().any(|n| n.tag == tag || tag_exists(&n.children, tag))
}

pub fn split_items(
    sidebar: &NSViewController,
    list: &NSViewController,
    note: &NSViewController,
    chat: &NSViewController,
) -> [Retained<NSSplitViewItem>; 4] {
    [
        NSSplitViewItem::sidebarWithViewController(sidebar),
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
