//! A window showing one vault: owns the AppKit objects and applies session events.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use knotes_app::{FolderNode, FolderRules, ListFilter, Sidebar, SidebarItem, TagNode, build_note_list, resolve_list_settings};
use knotes_core::{NoteRow, TagIndex};
use knotes_index::SqliteIndex;
use knotes_session::{Event, Snapshot};
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSView, NSViewController, NSWindow};
use objc2_foundation::{NSIndexSet, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString};

use crate::note_list_view::{self, NoteListSource};
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
    index_dir: Option<PathBuf>,
    /// Read-only index connection, opened on first need (the index exists after the first snapshot).
    reader: Option<SqliteIndex>,
}

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
    pub list: Retained<NoteListSource>,
    pub table: Retained<NSTableView>,
    /// Kept alive here: the toolbar holds its delegate weakly.
    _toolbar: Retained<ToolbarDelegate>,
    state: RefCell<State>,
    /// Set while the window selects a sidebar row itself, so the selection callback ignores it.
    highlighting: Cell<bool>,
}

impl VaultWindow {
    pub fn new(mtm: MainThreadMarker) -> Rc<Self> {
        let empty = Sidebar::new(&Default::default(), &[]);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&scroll);
        let list = NoteListSource::new(mtm);
        let (list_scroll, table) = note_list_view::table(mtm, &list);
        let list_vc = NSViewController::new(mtm);
        list_vc.setView(&list_scroll);
        let (window, split) = window::build(mtm, &sidebar_vc, &list_vc);
        let toolbar = ToolbarDelegate::new(mtm, split.splitView());
        toolbar::install(&window, &toolbar);
        let state = State {
            selection: SidebarItem::AllNotes,
            rows: Arc::new(Vec::new()),
            rules: Arc::new(HashMap::new()),
            folders: Arc::new(FolderNode::default()),
            tags: Arc::new(Vec::new()),
            index_dir: None,
            reader: None,
        };
        let this = Rc::new(Self {
            window,
            sidebar,
            outline,
            list,
            table,
            _toolbar: toolbar,
            state: RefCell::new(state),
            highlighting: Cell::new(false),
        });
        let weak: Weak<Self> = Rc::downgrade(&this);
        this.sidebar.on_select(move |item| {
            if let Some(window) = weak.upgrade() {
                window.select(item);
            }
        });
        this
    }

    pub fn set_index_dir(&self, dir: PathBuf) {
        self.state.borrow_mut().index_dir = Some(dir);
    }

    pub fn apply(&self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => self.show(&snapshot),
            Event::Error(e) => eprintln!("knotes: indexing stopped: {e}"),
        }
    }

    fn show(&self, snapshot: &Snapshot) {
        {
            let mut state = self.state.borrow_mut();
            state.rows = Arc::clone(&snapshot.rows);
            state.rules = Arc::clone(&snapshot.rules);
            state.folders = Arc::clone(&snapshot.folders);
            state.tags = Arc::clone(&snapshot.tags);
        }
        self.sidebar.set_model(Sidebar::new(&snapshot.folders, &snapshot.tags));
        self.outline.reloadData();
        for section in [SidebarItem::FoldersSection, SidebarItem::TagsSection] {
            // SAFETY: section objects come from the data source of this outline view.
            unsafe { self.outline.expandItem(Some(&self.sidebar.object_for(&section))) };
        }
        let mut selection = self.state.borrow().selection.clone();
        if !self.exists(&selection) {
            selection = SidebarItem::AllNotes;
            self.state.borrow_mut().selection = selection.clone();
        }
        self.highlight(&selection);
        self.refresh_list();
        if probe::enabled() && !snapshot.rows.is_empty() {
            if let Some(item) = probe::requested_selection() {
                self.highlight(&item);
                self.select(item);
            }
            probe::schedule(true);
        }
    }

    /// Shows the notes of a sidebar item.
    pub fn select(&self, item: SidebarItem) {
        if self.highlighting.get() {
            return;
        }
        self.state.borrow_mut().selection = item;
        self.refresh_list();
    }

    fn refresh_list(&self) {
        let mut state = self.state.borrow_mut();
        let selection = state.selection.clone();
        let (title, tagged) = match &selection {
            SidebarItem::Folder(path) => (path.rsplit('/').next().unwrap_or(path).to_owned(), None),
            SidebarItem::Tag(tag) => (format!("#{tag}"), Some(self.notes_with_tag(&mut state, tag))),
            _ => (strings::ALL_NOTES.to_owned(), None),
        };
        let filter = match (&selection, &tagged) {
            (SidebarItem::Folder(path), _) => ListFilter::Folder(path),
            (SidebarItem::Tag(_), Some(paths)) => ListFilter::Paths(paths),
            _ => ListFilter::All,
        };
        let folder = match &selection {
            SidebarItem::Folder(path) => path.as_str(),
            _ => "",
        };
        let settings = resolve_list_settings(folder, &state.rules);
        let items = build_note_list(&state.rows, &filter, &settings, &note_list_view::now());
        self.list.set_items(Arc::clone(&state.rows), items);
        drop(state);
        self.table.reloadData();
        self.window.setTitle(&NSString::from_str(&title));
        let count = self.list.note_count();
        let formatted = NSNumberFormatter::localizedStringFromNumber_numberStyle(
            &NSNumber::numberWithUnsignedLongLong(count as u64),
            NSNumberFormatterStyle::DecimalStyle,
        );
        self.window.setSubtitle(&NSString::from_str(&strings::notes_count(count, &formatted.to_string())));
    }

    fn notes_with_tag(&self, state: &mut State, tag: &str) -> HashSet<String> {
        if state.reader.is_none() {
            state.reader = state.index_dir.as_deref().and_then(|dir| SqliteIndex::open_reader(dir).ok());
        }
        state.reader.as_ref().and_then(|r| r.notes_with_tag(tag).ok()).unwrap_or_default().into_iter().collect()
    }

    fn exists(&self, item: &SidebarItem) -> bool {
        let state = self.state.borrow();
        match item {
            SidebarItem::Folder(path) => folder_exists(&state.folders, path),
            SidebarItem::Tag(tag) => tag_exists(&state.tags, tag),
            _ => true,
        }
    }

    /// Selects the row of `item` in the sidebar without notifying (the list follows `state`).
    fn highlight(&self, item: &SidebarItem) {
        // SAFETY: the item object comes from this outline view's data source.
        let row = unsafe { self.outline.rowForItem(Some(&self.sidebar.object_for(item))) };
        if row >= 0 {
            self.highlighting.set(true);
            self.outline.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(row as usize), false);
            self.highlighting.set(false);
        }
    }
}

fn folder_exists(node: &FolderNode, path: &str) -> bool {
    node.path == path || node.children.iter().any(|c| folder_exists(c, path))
}

fn tag_exists(nodes: &[TagNode], tag: &str) -> bool {
    nodes.iter().any(|n| n.tag == tag || tag_exists(&n.children, tag))
}

/// Unused split view items get an empty controller until their stage.
pub fn placeholder(mtm: MainThreadMarker) -> Retained<NSViewController> {
    let controller = NSViewController::new(mtm);
    controller.setView(&NSView::new(mtm));
    controller
}

pub fn split_items(sidebar: &NSViewController, list: &NSViewController, mtm: MainThreadMarker) -> [Retained<NSSplitViewItem>; 4] {
    [
        NSSplitViewItem::sidebarWithViewController(sidebar),
        NSSplitViewItem::contentListWithViewController(list),
        NSSplitViewItem::splitViewItemWithViewController(&placeholder(mtm)),
        NSSplitViewItem::inspectorWithViewController(&placeholder(mtm)),
    ]
}

/// Minimum and maximum width and holding priority per column: when the window resizes, the
/// note column stretches and the others keep their width. Explicit maximums: the inspector
/// role caps itself at 270 points otherwise.
const COLUMNS: [(f64, f64, f32); 4] = [(200.0, 480.0, 260.0), (260.0, 640.0, 255.0), (360.0, f64::MAX, 250.0), (260.0, 640.0, 255.0)];

pub fn split_controller(sidebar: &NSViewController, list: &NSViewController, mtm: MainThreadMarker) -> Retained<NSSplitViewController> {
    let split = NSSplitViewController::new(mtm);
    for (item, (min, max, holding)) in split_items(sidebar, list, mtm).iter().zip(COLUMNS) {
        item.setMinimumThickness(min);
        if max < f64::MAX {
            item.setMaximumThickness(max);
        }
        item.setHoldingPriority(holding);
        split.addSplitViewItem(item);
    }
    split
}
