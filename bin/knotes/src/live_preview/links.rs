//! Links (ADR 0013): a click follows one, footnotes lead to each other, a link's menu.

use super::*;

impl LivePreview {
    pub fn on_link_new_tab(&self, callback: impl Fn(&Link) + 'static) {
        *self.ivars().on_link_new_tab.borrow_mut() = Some(Box::new(callback));
    }

    /// The text's context menu for a right click at `point`, with "Відкрити в новій вкладці"
    /// first on a link to a document in the vault.
    pub fn link_context_menu(&self, view: &NSTextView, point: NSPoint, menu: Option<Retained<objc2_app_kit::NSMenu>>) -> Option<Retained<objc2_app_kit::NSMenu>> {
        let menu = menu.unwrap_or_else(|| objc2_app_kit::NSMenu::new(self.mtm()));
        self.add_link_items(view.characterIndexForInsertionAtPoint(point), &menu);
        Some(menu)
    }

    /// Adds the items for a link at character `index` (or just before it) to `menu`; false
    /// when there is none (no link there, or a web one).
    pub(super) fn add_link_items(&self, index: usize, menu: &objc2_app_kit::NSMenu) -> bool {
        let link = {
            let styled = self.ivars().styled.borrow();
            let image = |l: &&LinkRun| styled.images.iter().any(|i| i.range == l.range);
            link_at(&styled, index).or_else(|| link_at(&styled, index.checked_sub(1)?)).filter(|l| l.link.kind != LinkKind::External && !image(l)).map(|l| l.link.clone())
        };
        let Some(link) = link else { return false };
        *self.ivars().menu_link.borrow_mut() = Some(link);
        // SAFETY: an item sent to this preview, which outlives the menu; title and key are strings.
        let item = unsafe { objc2_app_kit::NSMenuItem::initWithTitle_action_keyEquivalent(objc2_app_kit::NSMenuItem::alloc(self.mtm()), &NSString::from_str(strings::OPEN_IN_NEW_TAB), Some(sel!(openLinkInNewTab:)), &NSString::new()) };
        // SAFETY: as above.
        unsafe { item.setTarget(Some(self)) };
        menu.insertItem_atIndex(&item, 0);
        if menu.numberOfItems() > 1 {
            menu.insertItem_atIndex(&objc2_app_kit::NSMenuItem::separatorItem(self.mtm()), 1);
        }
        true
    }

    /// `KNOTES_PROBE_LINK_MENU=index` (or `index:open`): a link's context menu at character
    /// `index`, its items printed (or "Відкрити в новій вкладці" chosen).
    pub fn probe_link_menu(&self, spec: &str) {
        let (index, open) = spec.split_once(':').map_or((spec, false), |(i, o)| (i, o == "open"));
        let menu = objc2_app_kit::NSMenu::new(self.mtm());
        if !index.parse().is_ok_and(|i| self.add_link_items(i, &menu)) {
            println!("link-menu: none");
            return;
        }
        let items: Vec<String> = menu.itemArray().iter().map(|i| i.title().to_string()).collect();
        println!("link-menu: {}", items.join(" | "));
        if open {
            self.open_menu_link();
        }
    }

    /// Opens the link the context menu was opened on in a new tab.
    pub(super) fn open_menu_link(&self) {
        let link = self.ivars().menu_link.borrow_mut().take();
        if let (Some(link), Some(callback)) = (link, self.ivars().on_link_new_tab.borrow().as_ref()) {
            callback(&link);
        }
    }

    /// A click on character `index`: follows a link where its markup is hidden, or anywhere
    /// with Cmd; elsewhere the click is the text view's (ADR 0013).
    pub fn click(&self, view: &NSTextView, index: usize, cmd: bool) -> bool {
        if !cmd && self.toggle_task_at(view, index) {
            return true;
        }
        if let Some(at) = self.footnote_target(index, cmd) {
            let at = at.min(view.string().length());
            view.setSelectedRange(NSRange::new(at, 0));
            view.scrollRangeToVisible(NSRange::new(at, 0));
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
        self.followed_link(index, false).is_some() || self.task_at(index).is_some() || self.footnote_target(index, false).is_some()
    }

    /// Where a click on character `index` goes, on a footnote's reference or definition label
    /// away from the cursor's paragraph (anywhere with Cmd): from a reference to just after its
    /// definition's `]:`, from a definition to just after its first reference (ADR 0034).
    pub(super) fn footnote_target(&self, index: usize, cmd: bool) -> Option<usize> {
        let styled = self.ivars().styled.borrow();
        let revealed = self.ivars().revealed.get();
        if !cmd && index >= revealed.0 && index < revealed.1 {
            return None;
        }
        let clicked = styled.footnotes.iter().find(|f| f.range.contains(&index))?;
        let other = styled.footnotes.iter().find(|f| f.definition != clicked.definition && f.label.eq_ignore_ascii_case(&clicked.label))?;
        Some(other.range.end)
    }

    /// The link a click on character `index` follows: where its markup is hidden, or anywhere
    /// with Cmd.
    pub(super) fn followed_link(&self, index: usize, cmd: bool) -> Option<Link> {
        let styled = self.ivars().styled.borrow();
        let revealed = self.ivars().revealed.get();
        let in_cursor_line = index >= revealed.0 && index < revealed.1;
        // An image is no link to follow: a click puts the cursor in its source.
        let image = |l: &&LinkRun| styled.images.iter().any(|i| i.range == l.range);
        link_at(&styled, index).filter(|l| (cmd || !in_cursor_line) && !image(l)).map(|link| link.link.clone())
    }
}

/// The link whose source range holds `index`.
pub(super) fn link_at(styled: &Highlight, index: usize) -> Option<&LinkRun> {
    let after = styled.links.partition_point(|l| l.range.start <= index);
    styled.links[..after].last().filter(|l| index < l.range.end)
}
