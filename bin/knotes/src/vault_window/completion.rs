//! The completion list while a link or a tag is typed (ADR 0013, 0033), in the note or an embedded one.

use super::*;

/// What the list under the cursor completes (ADR 0013, 0033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Completing {
    Link,
    /// A tag, in the text after `#` or in the frontmatter's list (no `#`, no space after).
    Tag { frontmatter: bool },
}

/// What the text before the cursor asks to complete.
pub(super) enum CompletionTarget {
    Link(LinkCompletion),
    Tag(TagCompletion),
}

/// A frontmatter longer than this (UTF-16) is not looked for: the note is read from its start
/// only up to here while typing.
pub(super) const FRONTMATTER_MOST: usize = 8192;

/// Paths of the `n` most recently modified notes, newest first.
pub(super) fn recent_notes(rows: &[NoteRow], n: usize) -> Vec<String> {
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

impl VaultWindow {
    /// Shows, updates or hides the link completion list for the text before the cursor.
    pub(super) fn update_completion(&self) {
        self.complete_in(&self.note_text, self.open_path());
    }

    /// The completion list for the cursor in `view`, which shows the note at `source` (the
    /// open note, or an embedded one: links are written as from it).
    pub(super) fn complete_in(&self, view: &NSTextView, source: Option<String>) {
        if self.accepting.get() {
            return;
        }
        let selection = view.selectedRange();
        let ns = view.string();
        let found = (selection.length == 0).then(|| self.completion_at(&ns, selection.location)).flatten();
        let Some((anchor, from, target)) = found else {
            self.completion.hide();
            self.completion_dismissed.set(None);
            return;
        };
        if self.completion_dismissed.get() == Some(anchor) {
            return;
        }
        let (items, completing) = match &target {
            CompletionTarget::Link(link) => (source.map(|source| self.completion_items(link, &source, view)).unwrap_or_default(), Completing::Link),
            CompletionTarget::Tag(tag) => (self.tag_items(tag), Completing::Tag { frontmatter: tag.frontmatter }),
        };
        // The tag typed in full and nothing else to offer: Return makes a new line again.
        let typed_in_full = matches!((&target, items.as_slice()), (CompletionTarget::Tag(tag), [only]) if only.insert == tag.query);
        if items.is_empty() || typed_in_full {
            self.completion.hide();
            return;
        }
        self.completion_from.set(from);
        self.completion_anchor.set(anchor);
        self.completing.set(completing);
        *self.completion_view.borrow_mut() = Some(objc2::Message::retain(view));
        // SAFETY: the range lies inside the text; a null actual range is allowed.
        let cursor = unsafe { view.firstRectForCharacterRange_actualRange(NSRange::new(selection.location, 0), std::ptr::null_mut()) };
        self.completion.show(&self.window, cursor, items);
    }

    /// What the cursor at `cursor` (UTF-16) completes: a link inside `[[`, or a tag (in the text
    /// after `#`, or in the frontmatter's tags); with where its list begins and where the
    /// replaced part starts (UTF-16).
    pub(super) fn completion_at(&self, ns: &NSString, cursor: usize) -> Option<(usize, usize, CompletionTarget)> {
        let line = ns.paragraphRangeForRange(NSRange::new(cursor, 0));
        let before = ns.substringWithRange(NSRange::new(line.location, cursor - line.location)).to_string();
        let units = |text: &str, start: usize, bytes: usize| start + text[..bytes].encode_utf16().count();
        if let Some(link) = link_completion(&before) {
            let open = units(&before, line.location, before.rfind("[[").unwrap_or(0));
            let from = match &link {
                LinkCompletion::Title { from, .. } | LinkCompletion::Heading { from, .. } | LinkCompletion::Block { from, .. } => *from,
            };
            return Some((open, units(&before, line.location, from), CompletionTarget::Link(link)));
        }
        // The note is read from its start only for a tag after `#`, or in a frontmatter.
        let in_frontmatter = cursor <= FRONTMATTER_MOST && ns.hasPrefix(&NSString::from_str("---"));
        if !tag_typed(&before) && !in_frontmatter {
            return None;
        }
        let head = ns.substringWithRange(NSRange::new(0, cursor)).to_string();
        let tag = tag_completion(&head)?;
        let from = units(&head, 0, tag.from);
        Some((from, from, CompletionTarget::Tag(tag)))
    }

    /// The tags to offer for the one typed (ADR 0033): in a space its own first.
    pub(super) fn tag_items(&self, tag: &TagCompletion) -> Vec<CompletionItem> {
        let state = self.state.borrow();
        let space = state.space.as_ref().and_then(|id| state.space_tags.iter().find(|counted| counted.id == *id));
        suggest_tags(&tag.query, space.map_or(&[][..], |counted| counted.tags.as_slice()), &state.tags, MAX_ROWS)
            .into_iter()
            .map(|(name, notes)| CompletionItem {
                title: if tag.frontmatter { name.to_owned() } else { format!("#{name}") },
                detail: notes.to_string(),
                insert: name.to_owned(),
            })
            .collect()
    }

    pub(super) fn completion_items(&self, completion: &LinkCompletion, source: &str, view: &NSTextView) -> Vec<CompletionItem> {
        let state = self.state.borrow();
        let (Some(links), Some(titles)) = (state.links.as_ref(), state.titles.as_ref()) else {
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
                        let link = links.link_text(&path, source);
                        CompletionItem {
                            title: alias.clone().unwrap_or_else(|| name.to_owned()),
                            detail: folder.to_owned(),
                            insert: alias.map_or_else(|| link.clone(), |a| format!("{link}|{a}")),
                        }
                    })
                    .collect()
            }
            LinkCompletion::Heading { target, query, .. } | LinkCompletion::Block { target, query, .. } => {
                let Some(path) = links.resolve(LinkKind::Wiki, target, source) else { return Vec::new() };
                let text = if path == source {
                    view.string().to_string()
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
    pub(super) fn completion_command(&self, command: Sel) -> bool {
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
            self.completion_dismissed.set(Some(self.completion_anchor.get()));
        } else {
            return false;
        }
        true
    }

    /// Replaces the typed part with the chosen link (or heading), closing it with `]]` unless
    /// the brackets already follow, and puts the cursor after them; or with the chosen tag, a
    /// space after it in the text unless one follows (none in the frontmatter's list).
    pub(super) fn accept_completion(&self) {
        let Some(item) = self.completion.chosen() else { return };
        self.completion.hide();
        let view = self.completion_view.borrow().clone().unwrap_or_else(|| Retained::into_super(self.note_text.clone()));
        let cursor = view.selectedRange().location;
        let from = self.completion_from.get().min(cursor);
        let ns = view.string();
        let closed = cursor + 2 <= ns.length() && ns.substringWithRange(NSRange::new(cursor, 2)).to_string() == "]]";
        let spaced = cursor < ns.length() && matches!(ns.characterAtIndex(cursor), 0x20 | 0x09 | 0x0A | 0x0D);
        let (text, closed) = match self.completing.get() {
            Completing::Link if closed => (item.insert, true),
            Completing::Link => (format!("{}]]", item.insert), false),
            Completing::Tag { frontmatter: false } if !spaced => (format!("{} ", item.insert), false),
            Completing::Tag { .. } => (item.insert, false),
        };
        self.accepting.set(true);
        view.setSelectedRange(NSRange::new(from, cursor - from));
        let not_found = NSRange::new(objc2_foundation::NSNotFound as usize, 0);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { NSTextInputClient::insertText_replacementRange(&*view, &NSString::from_str(&text), not_found) };
        let after = from + text.encode_utf16().count() + if closed { 2 } else { 0 };
        view.setSelectedRange(NSRange::new(after, 0));
        self.accepting.set(false);
    }

    pub fn describe_completion(&self) -> String {
        self.completion.describe()
    }
}
