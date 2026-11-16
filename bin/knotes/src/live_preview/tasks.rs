//! Task boxes (ADR 0018): ticked with a click, made from `- []` and a space.

use super::*;

impl LivePreview {
    /// The task marker whose drawn circle holds character `index` (not one showing its source).
    pub(super) fn task_at(&self, index: usize) -> Option<Run> {
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
    pub(super) fn toggle_task_at(&self, view: &NSTextView, index: usize) -> bool {
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
    pub(super) fn complete_task_box(&self, view: &NSTextView, space: usize) {
        // In a code block it is an example, kept as typed.
        if self.ivars().styled.borrow().code_blocks.iter().any(|block| block.start <= space && space < block.end) {
            return;
        }
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
}
