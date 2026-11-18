//! What paragraphs draw in place of hidden markup: list markers, callouts, hidden runs, the previews below formulas, quote bars.

use super::*;

impl LivePreview {
    /// A list item's marker and its paragraph's indentation. A bullet's `- ` or a task's
    /// `- [ ] ` is hidden, its room kept (by kerning) for the dot or box the fragment draws,
    /// unless the selection touches it; numbers show. The source's indentation is hidden: each level indents by `LIST_INDENT`,
    /// and wrapped lines line up with the item's text.
    pub(super) fn paint_list_marker<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, run: &Run) {
        let Style::ListMarker { kind, depth } = run.style else { return };
        let look = &self.ivars().look;
        let text = storage.string();
        let mut line_start = run.range.start;
        while line_start > 0 && matches!(text.characterAtIndex(line_start - 1), 0x20 | 0x09) {
            line_start -= 1;
        }
        // At a line's start, or right after a quote's `>`.
        let blank = line_start == 0 || matches!(text.characterAtIndex(line_start - 1), 0x0a | 0x0d | 0x3e);
        let hidden = kind != ListKind::Ordered && !touches(&run.range, self.ivars().selection.get());
        let font = if kind == ListKind::Ordered { &look.digits } else { &look.body };
        let hidden_room = if kind == ListKind::Bullet { BULLET_ROOM } else { TASK_ROOM };
        let room = if hidden {
            hidden_room
        } else if kind == ListKind::Ordered {
            self.number_width(&text, &run.range)
        } else {
            text_width(&text.substringWithRange(ns(&run.range)), font)
        };
        let paragraph = paragraph(storage, NSRange::new(run.range.start, 0));
        // A quote's indentation stays.
        let base = palette.para_of(canvas.get(paragraph.0).map_or(0, |p| p.para));
        let start = base.first_indent + if blank { f64::from(depth) * LIST_INDENT } else { 0.0 };
        // An empty item (its marker alone) keeps a line's height, as an empty quote line does.
        let min_line = base.min_line.max(self.line_height());
        let para = palette.para(Para { first_indent: start, head_indent: start + room, hidden_tabs: blank, min_line, ..base });
        canvas.paint(paragraph.0..paragraph.1, |p| p.para = para);
        if blank {
            let id = palette.look(&look.hidden, &look.clear, "indent");
            canvas.paint(line_start..run.range.start, |p| p.look = id);
        }
        if hidden {
            self.paint_hidden(canvas, palette, storage, &run.range, if kind == ListKind::Bullet { "bullet" } else { "task" }, hidden_room);
        } else {
            let id = palette.look(font, &look.muted, "list-marker");
            canvas.paint(run.range.clone(), |p| p.look = id);
        }
    }

    /// A callout's `[!kind]`, hidden (its room kept for the icon, and the kind's name when no
    /// title is written) unless the cursor is on its line; its title bold in its colour.
    pub(super) fn paint_callout<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, callout: &CalloutRun, revealed: bool) {
        let look = &self.ivars().look;
        let (colour, _) = callout_look(&callout.kind);
        let colour = &look.callouts[colour];
        if revealed {
            let id = palette.look(&look.body, &look.muted, "callout-marker");
            canvas.paint(callout.marker.clone(), |p| p.look = id);
        } else {
            let label = if callout.title.is_empty() { text_width(&NSString::from_str(&callout_name(&callout.kind)), &look.bold) + 4.0 } else { 0.0 };
            self.paint_hidden(canvas, palette, storage, &callout.marker, "callout", CALLOUT_ICON_ROOM + label);
        }
        let title = palette.look(&look.bold, colour, "callout-title");
        canvas.paint(callout.title.clone(), |p| p.look = title);
    }

    /// Hides the source in `range`, keeping `room` points for what the fragment draws there.
    /// The last character keeps the body font (clear): a line of tiny hidden glyphs alone sat
    /// low, and the cursor after them was drawn askew until the first key. Its width is taken
    /// back by kerning on the first character (kerning on a line's last character is dropped),
    /// or on itself when there is no room to keep.
    pub(super) fn paint_hidden<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, range: &Range<usize>, name: &'static str, room: f64) {
        let Some(last) = range.end.checked_sub(1).filter(|&l| l >= range.start) else { return };
        let look = &self.ivars().look;
        let width = self.body_width(storage.string().characterAtIndex(last));
        let kern = palette.kern(room - width);
        let hidden = palette.look(&look.hidden, &look.clear, name);
        canvas.paint(range.start..last, |p| {
            p.look = hidden;
            p.kern = 0;
        });
        let body = palette.look(&look.body, &look.clear, name);
        canvas.paint(last..range.end, |p| {
            p.look = body;
            p.kern = 0;
        });
        // Taking width back (no room, a quote's `> `) works on the last character only: a tiny
        // glyph cannot go below no advance at all.
        let at = if room < width { last } else { range.start };
        canvas.paint(at..at + 1, |p| p.kern = kern);
    }

    /// Width of a character in the body font, once measured; a tab's is next to nothing where
    /// the source's indentation is hidden.
    pub(super) fn body_width(&self, unit: u16) -> f64 {
        if unit == 0x09 {
            return 0.0;
        }
        let look = &self.ivars().look;
        // Between two letters: a lone space measures as nothing (trailing white space).
        *look.body_widths.borrow_mut().entry(unit).or_insert_with(|| {
            let between = format!("x{}x", String::from_utf16_lossy(&[unit]));
            text_width(&NSString::from_str(&between), &look.body) - text_width(&NSString::from_str("xx"), &look.body)
        })
    }

    /// Height of a line of body text: what a line of hidden source alone would not get.
    pub(super) fn line_height(&self) -> f64 {
        let body = &self.ivars().look.body;
        body.ascender() - body.descender() + body.leading()
    }

    /// Width of a list number (`12. `) in the digits font, without measuring the text.
    pub(super) fn number_width(&self, text: &NSString, range: &Range<usize>) -> f64 {
        let look = &self.ivars().look;
        let widths = look.number_widths.get().unwrap_or_else(|| {
            let width = |s: &str| text_width(&NSString::from_str(s), &look.digits);
            let w = [width("0"), width("."), width(")"), width(" ")];
            look.number_widths.set(Some(w));
            w
        });
        range
            .clone()
            .map(|i| match text.characterAtIndex(i) {
                0x30..=0x39 => widths[0],
                0x2e => widths[1],
                0x29 => widths[2],
                0x20 => widths[3],
                _ => 0.0,
            })
            .sum()
    }

    /// What the paragraph at `range` draws: each typeset formula starting in it (inline, or a
    /// display formula on its own line), the preview of a display formula with the cursor in
    /// it below its last paragraph, the dot or circle of a hidden bullet or task marker, a
    /// quote's bars and a code block's background.
    pub(super) fn preview_marks(&self, layout: &NSTextLayoutManager, range: Option<&NSTextRange>) -> Vec<Mark> {
        let styled = self.ivars().styled.borrow();
        let (Some(range), false) = (range, styled.formulas.is_empty() && styled.runs.is_empty() && styled.code_blocks.is_empty() && styled.tables.is_empty()) else {
            return Vec::new();
        };
        let Some(content) = layout.textContentManager() else { return Vec::new() };
        let storage = content.downcast_ref::<NSTextContentStorage>().and_then(|c| c.textStorage());
        let Some(storage) = storage else { return Vec::new() };
        let start = content.documentRange().location();
        let from = content.offsetFromLocation_toLocation(&start, &range.location()).max(0) as usize;
        let to = content.offsetFromLocation_toLocation(&start, &range.endLocation()).max(0) as usize;
        let revealed = self.ivars().revealed.get();
        let first = styled.formulas.partition_point(|f| f.range.end <= from);
        let mut marks = Vec::new();
        for f in styled.formulas[first..].iter().take_while(|f| f.range.start < to) {
            let block = f.display && is_block(&storage, f);
            let (anchor, at) = match (overlaps(&f.range, revealed), block) {
                (false, true) => (Anchor::Line, f.range.start),
                (false, false) => (Anchor::Inline, f.range.start),
                (true, true) => (Anchor::Below, f.range.end.saturating_sub(1)),
                (true, false) => continue,
            };
            if !(from..to).contains(&at) {
                continue;
            }
            let Ok(image) = self.formula_image(f) else { continue };
            marks.push(Mark::Formula { offset: at - from, image, anchor });
        }
        for run in styled.images.iter().filter(|i| i.range.start < to && i.range.end > from) {
            let Some((image, size)) = self.image(run) else { continue };
            let (anchor, at) = if overlaps(&run.range, revealed) {
                (Anchor::Below, run.range.end.saturating_sub(1))
            } else {
                (Anchor::Line, paragraph(&storage, ns(&run.range)).0)
            };
            if (from..to).contains(&at) {
                let handle = anchor == Anchor::Line && self.ivars().hovered_image.get() == Some(paragraph(&storage, ns(&run.range)).0);
                marks.push(Mark::Image { offset: at - from, image, size, anchor, handle, caption: self.image_caption(run) });
            }
        }
        if !styled.embeds.is_empty() {
            let heights = self.ivars().embed_heights.borrow();
            for (key, _, range) in self.embeds(&storage.string()).into_iter().filter(|(_, _, r)| r.0 < to && r.1 > from) {
                let Some(&height) = heights.get(&key) else { continue };
                let below = overlaps(&(range.0..range.1), revealed);
                let at = if below { range.1.saturating_sub(1) } else { paragraph(&storage, NSRange::new(range.0, range.1 - range.0)).0 };
                if (from..to).contains(&at) {
                    marks.push(Mark::EmbedBox { below, height });
                }
            }
        }
        let centre_x = self.ivars().look.body.xHeight() / 2.0;
        let text = storage.string();
        for end in external_link_ends(&styled, (from, to), self.ivars().revealed.get()) {
            // At the hidden `]`, whose own room follows it; or where a bare address ends, whose
            // location sits halfway into the room kerned after the address's last character
            // (where a caret would go) — unless the line ends there and the kerning is dropped.
            let (offset, shift) = if end.bare {
                let line_end = end.kern_at + 1 >= text.length() || matches!(text.characterAtIndex(end.kern_at + 1), 0x0a | 0x0d);
                (end.kern_at + 1, if line_end { 3.0 } else { 3.0 - EXTERNAL_ROOM / 2.0 })
            } else {
                (end.kern_at, 3.0)
            };
            if offset >= from && offset <= to {
                marks.push(Mark::External { offset: offset - from, centre: centre_x + 1.5, shift });
            }
        }
        let revealed = self.ivars().revealed.get();
        let look = &self.ivars().look;
        for callout in styled.callouts.iter().filter(|c| c.range.start < to && c.range.end > from) {
            let (colour, symbol) = callout_look(&callout.kind);
            let colour = look.callouts[colour].clone();
            marks.push(Mark::CalloutBox { colour: colour.clone(), top: from <= callout.range.start, bottom: to >= callout.range.end });
            if callout.marker.start >= from && callout.marker.start < to && !overlaps(&callout.marker, revealed) {
                let label = callout.title.is_empty().then(|| callout_name(&callout.kind));
                marks.push(Mark::CalloutIcon { offset: callout.marker.start - from, centre: look.body.capHeight() / 2.0, colour, symbol, label, font: look.bold.clone() });
            }
        }
        for (t, table) in styled.tables.iter().enumerate().filter(|(_, t)| t.range.start < to && t.range.end > from) {
            let Some(columns) = self.ivars().table_columns.borrow().get(&table.range.start).cloned() else { continue };
            let scrolled = self.ivars().table_scroll.borrow().get(&t).is_some_and(|&scroll| scroll > 0.0);
            let hover = self.ivars().hovered_table.get().filter(|h| h.0 == table.range.start);
            let last = table.rows.len().saturating_sub(1);
            for (i, _) in table.rows.iter().enumerate().filter(|(_, r)| (from..to).contains(&r.range.start)) {
                let controls = hover.map_or(TableControls::default(), |(_, row, column)| TableControls {
                    handle: row == Some(i),
                    column_handle: if i == 0 { column } else { None },
                    add_row: i == last,
                    add_column: true,
                    add_column_plus: i == last / 2,
                    first: i == 0,
                    last: i == last,
                });
                marks.push(Mark::TableRow { columns: Rc::clone(&columns), header: i == 0, top: i == 0, scrolled, controls });
            }
        }
        // A card's bar down each of its paragraphs, the blank ones inside braces too.
        let card = styled.cards.partition_point(|c| c.end < from);
        if styled.cards.get(card).is_some_and(|c| c.start < to && (c.end > from || c.end == from && from == to)) {
            marks.push(Mark::QuoteBar { x: 0.0, card: Some(look.card.clone()) });
        }
        let block = styled.code_blocks.partition_point(|c| c.end <= from);
        if let Some(c) = styled.code_blocks.get(block).filter(|c| c.start < to) {
            marks.push(Mark::CodeBackground { top: from <= c.start, bottom: to >= c.end });
            // A code line's number (ADR 0025); the fences have none.
            let lines = self.code_lines(&storage, &styled, c);
            if let Ok(n) = lines.binary_search(&from) {
                marks.push(Mark::CodeNumber { number: (n + 1).to_string(), font: self.ivars().look.mono.clone() });
            }
            // The first code line: the language (its fence hidden) and, with the pointer over
            // the block, the copy button.
            if lines.first() == Some(&from) {
                let label = (!overlaps(c, revealed))
                    .then(|| styled.code_langs.iter().find(|r| r.block == *c).and_then(|r| r.language.split_whitespace().next()).map(|l| l.trim_start_matches(['{', '.']).to_owned()))
                    .flatten();
                let copy = self.ivars().hovered_code.get() == Some(c.start);
                if label.is_some() || copy {
                    marks.push(Mark::CodeCorner { label, copy });
                }
            }
        }
        let selection = self.ivars().selection.get();
        // A dot on the middle of small letters; a circle, as tall as capitals, on theirs.
        let centre = self.ivars().look.body.xHeight() / 2.0;
        let task_centre = self.ivars().look.body.capHeight() / 2.0;
        let first = styled.runs.partition_point(|r| r.range.start < from);
        for run in styled.runs[first..].iter().take_while(|r| r.range.start < to) {
            // A thematic break away from the cursor: a line in the place of its signs.
            if run.style == Style::Rule {
                if !overlaps(&run.range, revealed) {
                    marks.push(Mark::Rule);
                }
                continue;
            }
            if let Style::QuoteMarker { depth } = run.style {
                // A callout's lines show its box instead of bars.
                if !styled.callouts.iter().any(|c| run.range.start >= c.range.start && run.range.start < c.range.end) {
                    marks.extend((0..depth).map(|level| Mark::QuoteBar { x: f64::from(level) * QUOTE_INDENT, card: None }));
                }
                continue;
            }
            let Style::ListMarker { kind, .. } = run.style else { continue };
            if touches(&run.range, selection) {
                continue;
            }
            let offset = run.range.start - from;
            match kind {
                ListKind::Bullet => marks.push(Mark::Bullet { offset, centre }),
                ListKind::Task { checked } => marks.push(Mark::Task { offset, centre: task_centre, checked }),
                ListKind::Ordered => {}
            }
        }
        marks
    }
}

/// Room a hidden callout marker keeps for its icon.
pub(super) const CALLOUT_ICON_ROOM: f64 = 20.0;

/// A callout kind's colour (index into the look's callout colours) and icon, as Obsidian's.
pub(super) fn callout_look(kind: &str) -> (usize, &'static str) {
    match kind {
        "abstract" | "summary" | "tldr" => (1, "list.bullet.clipboard"),
        "info" => (0, "info.circle"),
        "todo" => (0, "checkmark.circle"),
        "tip" | "hint" | "important" => (1, "flame"),
        "success" | "check" | "done" => (2, "checkmark"),
        "question" | "help" | "faq" => (3, "questionmark.circle"),
        "warning" | "caution" | "attention" => (3, "exclamationmark.triangle"),
        "failure" | "fail" | "missing" => (4, "xmark"),
        "danger" | "error" => (4, "bolt"),
        "bug" => (4, "ladybug"),
        "example" => (5, "list.bullet"),
        "quote" | "cite" => (6, "quote.opening"),
        _ => (0, "pencil"),
    }
}

/// A callout's name when no title is written: its kind, capitalised (as Obsidian shows it).
pub(super) fn callout_name(kind: &str) -> String {
    let mut chars = kind.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
