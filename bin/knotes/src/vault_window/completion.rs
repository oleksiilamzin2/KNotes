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
        knotes_app::offer_tags(&tag.query, tag.frontmatter, space.map_or(&[][..], |counted| counted.tags.as_slice()), &state.tags, MAX_ROWS)
    }

    pub(super) fn completion_items(&self, completion: &LinkCompletion, source: &str, view: &NSTextView) -> Vec<CompletionItem> {
        let state = self.state.borrow();
        let (Some(links), Some(titles)) = (state.links.as_ref(), state.titles.as_ref()) else {
            return Vec::new();
        };
        match completion {
            LinkCompletion::Title { query, .. } => {
                let found: Vec<(String, Option<String>)> = if query.is_empty() {
                    knotes_app::recent_notes(&state.rows, MAX_ROWS).into_iter().map(|p| (p, None)).collect()
                } else {
                    titles.search_titles(query, MAX_ROWS).into_iter().map(|h| (h.path, h.alias)).collect()
                };
                knotes_app::offer_notes(found, source, &**links)
            }
            LinkCompletion::Heading { target, query, .. } | LinkCompletion::Block { target, query, .. } => {
                let Some(path) = links.resolve(LinkKind::Wiki, target, source) else { return Vec::new() };
                let text = if path == source {
                    view.string().to_string()
                } else {
                    state.vault.as_ref().and_then(|v| FsVault::new(v).read_text(&path).ok()).unwrap_or_default()
                };
                if matches!(completion, LinkCompletion::Block { .. }) {
                    return knotes_app::offer_blocks(&text, MarkdownParser::new().parse(&text).anchors, query, MAX_ROWS);
                }
                knotes_app::offer_headings(note_headings(&text), query, MAX_ROWS)
            }
        }
    }

    /// Keys while the list is open: Up/Down choose, Return/Tab insert, Esc closes. Tab on a tag
    /// with tags nested under it steps into it instead: `tag/` and the tags of that level.
    pub(super) fn completion_command(&self, command: Sel) -> bool {
        if !self.completion.is_visible() {
            return false;
        }
        if command == sel!(moveDown:) {
            self.completion.move_selection(1);
        } else if command == sel!(moveUp:) {
            self.completion.move_selection(-1);
        } else if command == sel!(insertNewline:) {
            self.accept_completion(false);
        } else if command == sel!(insertTab:) {
            self.accept_completion(true);
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
    /// space after it in the text unless one follows (none in the frontmatter's list). With
    /// `step`, a tag that has tags nested under it goes in with `/` after it, and the list
    /// shows those.
    pub(super) fn accept_completion(&self, step: bool) {
        let Some(item) = self.completion.chosen() else { return };
        let step = step && matches!(self.completing.get(), Completing::Tag { .. }) && {
            let state = self.state.borrow();
            knotes_app::has_nested_tags(&state.tags, &item.insert) || state.space_tags.iter().any(|counted| knotes_app::has_nested_tags(&counted.tags, &item.insert))
        };
        self.completion.hide();
        let view = self.completion_view.borrow().clone().unwrap_or_else(|| Retained::into_super(self.note_text.clone()));
        let cursor = view.selectedRange().location;
        let from = self.completion_from.get().min(cursor);
        let ns = view.string();
        let closed = cursor + 2 <= ns.length() && ns.substringWithRange(NSRange::new(cursor, 2)).to_string() == "]]";
        let spaced = cursor < ns.length() && matches!(ns.characterAtIndex(cursor), 0x20 | 0x09 | 0x0A | 0x0D);
        let (text, closed) = match self.completing.get() {
            Completing::Tag { .. } if step => (format!("{}/", item.insert), false),
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
        if step {
            self.complete_in(&view, self.open_path());
        }
    }

    pub fn describe_completion(&self) -> String {
        self.completion.describe()
    }
}
