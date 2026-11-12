//! A window showing one vault: owns the AppKit objects and applies session events.

use knotes_app::{Sidebar, SidebarItem};
use knotes_session::{Event, Snapshot};
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSOutlineView, NSSplitViewController, NSSplitViewItem, NSView, NSViewController, NSWindow};

use crate::sidebar_view::{self, SidebarSource};
use crate::{probe, window};

pub struct VaultWindow {
    pub window: Retained<NSWindow>,
    pub sidebar: Retained<SidebarSource>,
    pub outline: Retained<NSOutlineView>,
}

impl VaultWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let empty = Sidebar::new(&Default::default(), &[]);
        let sidebar = SidebarSource::new(mtm, empty);
        let (scroll, outline) = sidebar_view::outline(mtm, &sidebar);
        let sidebar_vc = NSViewController::new(mtm);
        sidebar_vc.setView(&scroll);
        let window = window::build(mtm, &sidebar_vc);
        Self { window, sidebar, outline }
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

pub fn split_items(sidebar: &NSViewController, mtm: MainThreadMarker) -> [Retained<NSSplitViewItem>; 4] {
    [
        NSSplitViewItem::sidebarWithViewController(sidebar),
        NSSplitViewItem::contentListWithViewController(&placeholder(mtm)),
        NSSplitViewItem::splitViewItemWithViewController(&placeholder(mtm)),
        NSSplitViewItem::inspectorWithViewController(&placeholder(mtm)),
    ]
}

pub fn split_controller(sidebar: &NSViewController, mtm: MainThreadMarker) -> Retained<NSSplitViewController> {
    let split = NSSplitViewController::new(mtm);
    for item in &split_items(sidebar, mtm) {
        split.addSplitViewItem(item);
    }
    split
}
