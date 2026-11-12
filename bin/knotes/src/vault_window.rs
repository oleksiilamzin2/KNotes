//! A window showing one vault: owns the AppKit objects and applies session events.

use knotes_app::{ListFilter, ListSettings, Sidebar, SidebarItem, build_note_list};
use knotes_session::{Event, Snapshot};
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSOutlineView, NSSplitViewController, NSSplitViewItem, NSTableView, NSView, NSViewController, NSWindow};
use objc2_foundation::{NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString};

use crate::note_list_view::{self, NoteListSource};
use crate::sidebar_view::{self, SidebarSource};
use crate::{probe, strings, window};

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
    pub list: Retained<NoteListSource>,
    pub table: Retained<NSTableView>,
}

impl VaultWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let empty = Sidebar::new(&Default::default(), &[]);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&scroll);
        let list = NoteListSource::new(mtm);
        let (list_scroll, table) = note_list_view::table(mtm, &list);
        let list_vc = NSViewController::new(mtm);
        list_vc.setView(&list_scroll);
        let window = window::build(mtm, &sidebar_vc, &list_vc);
        Self { window, sidebar, outline, list, table }
    }

    pub fn apply(&self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => self.show(&snapshot),
            Event::Error(e) => eprintln!("knotes: indexing stopped: {e}"),
        }
    }

    fn show(&self, snapshot: &Snapshot) {
        self.sidebar.set_model(Sidebar::new(&snapshot.folders, &snapshot.tags));
        self.outline.reloadData();
        for section in [SidebarItem::FoldersSection, SidebarItem::TagsSection] {
            // SAFETY: section objects come from the data source of this outline view.
            unsafe { self.outline.expandItem(Some(&self.sidebar.object_for(&section))) };
        }
        let items = build_note_list(&snapshot.rows, &ListFilter::All, &ListSettings::default(), &note_list_view::now());
        self.list.set_items(snapshot.rows.clone(), items);
        self.table.reloadData();
        self.window.setTitle(&NSString::from_str(strings::ALL_NOTES));
        let count = self.list.note_count();
        let formatted = NSNumberFormatter::localizedStringFromNumber_numberStyle(
            &NSNumber::numberWithUnsignedLongLong(count as u64),
            NSNumberFormatterStyle::DecimalStyle,
        );
        self.window.setSubtitle(&NSString::from_str(&strings::notes_count(count, &formatted.to_string())));
        if probe::enabled() && !snapshot.rows.is_empty() {
            probe::schedule(true);
        }
    }
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

pub fn split_controller(sidebar: &NSViewController, list: &NSViewController, mtm: MainThreadMarker) -> Retained<NSSplitViewController> {
    let split = NSSplitViewController::new(mtm);
    for item in &split_items(sidebar, list, mtm) {
        split.addSplitViewItem(item);
    }
    split
}
