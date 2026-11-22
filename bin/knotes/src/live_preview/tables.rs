//! Tables (ADR 0021): drawn as grids, edited in place (the structure kept), their menus, controls and sideways scrolling.

use super::*;

impl LivePreview {
    /// A table as a grid (ADR 0021): each cell's text moved to its column by kerning on the
    /// hidden pipes and spaces before it, columns as wide as their widest cell, the delimiter
    /// row folded away, rows padded; the fragments draw the borders.
    pub(super) fn paint_table<'a>(&'a self, canvas: &mut Canvas<Paint>, palette: &mut Palette<'a>, storage: &NSTextStorage, t: usize, table: &TableRun) {
        let text = storage.string();
        let columns = table.aligns.len().max(table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0));
        let widths: Vec<Vec<f64>> = table.rows.iter().map(|row| row.cells.iter().map(|c| self.shown_width(canvas, palette, &text, c)).collect()).collect();
        let mut column_widths = vec![TABLE_MIN_COLUMN; columns];
        for row in &widths {
            for (w, column) in row.iter().zip(column_widths.iter_mut()) {
                *column = column.max(*w);
            }
        }
        let padded: Vec<f64> = column_widths.iter().map(|w| w + 2.0 * TABLE_PAD).collect();
        // A wide table scrolled sideways by `scroll` points, no further than its right edge.
        let total: f64 = padded.iter().sum();
        let scroll = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0.0).clamp(0.0, (total - self.ivars().column.get()).max(0.0));
        if scroll > 0.0 {
            self.ivars().table_scroll.borrow_mut().insert(t, scroll);
        } else {
            self.ivars().table_scroll.borrow_mut().remove(&t);
        }
        let laid_out = (padded.clone(), scroll.to_bits(), table.range.start);
        if self.ivars().table_layouts.borrow_mut().insert(t, laid_out.clone()).as_ref() != Some(&laid_out) {
            self.ivars().tables_changed.set(true);
        }
        let mut edges = Vec::with_capacity(columns + 1);
        let mut x = -scroll;
        for w in &padded {
            edges.push(x);
            x += w;
        }
        edges.push(x);
        self.ivars().table_widths.borrow_mut().insert(t, padded);
        self.ivars().table_columns.borrow_mut().insert(table.range.start, Rc::new(edges.clone()));
        let hidden = palette.look(&self.look().hidden, &self.look().clear, "table");
        for (row, widths) in table.rows.iter().zip(&widths) {
            // `from` starts the hidden source still to place before the next text shown, `at`
            // is where the text shown so far ends.
            let (mut at, mut from, mut first_indent) = (0.0, row.range.start, 0.0);
            for (j, (cell, width)) in row.cells.iter().zip(widths).enumerate() {
                let room = column_widths[j] - width;
                let offset = match table.aligns.get(j) {
                    Some(Align::Center) => room / 2.0,
                    Some(Align::Right) => room,
                    _ => 0.0,
                };
                let x = edges[j] + TABLE_PAD + offset;
                if x + width <= 0.0 {
                    // Scrolled away whole: hidden with the pipes around it.
                    canvas.paint(cell.clone(), |p| {
                        p.look = hidden;
                        p.kern = 0;
                    });
                    continue;
                }
                // Cut by the table's left edge: the characters before it hidden, the first one
                // after it where it would be (glyphs cannot go left of a line's start).
                let (shown, shown_x) = if x < 0.0 {
                    let (cut, before) = self.cut_at(canvas, palette, &text, cell, -x);
                    (cut, x + before)
                } else {
                    (cell.start, x)
                };
                if shown > from {
                    self.paint_hidden(canvas, palette, storage, &(from..shown), "table", shown_x - at);
                } else {
                    // No pipe before the first cell: the line starts there.
                    first_indent = shown_x;
                }
                at = x + width;
                from = cell.end;
            }
            if from < row.range.end {
                self.paint_hidden(canvas, palette, storage, &(from..row.range.end), "table", 0.0);
            }
            // Padding above as a taller line (spacing before the document's first paragraph is
            // dropped), below as spacing.
            let para = palette.para(Para { first_indent, unwrapped: true, min_line: self.line_height() + TABLE_VPAD, spacing_after: TABLE_VPAD, ..Para::default() });
            canvas.paint(row.range.start..row.range.end + 1, |p| p.para = para);
        }
        canvas.paint(table.delimiter.clone(), |p| {
            p.look = hidden;
            p.kern = 0;
        });
        let folded = palette.para(Para { max_line: 0.01, ..Para::default() });
        canvas.paint(table.delimiter.start..table.delimiter.end + 1, |p| p.para = folded);
    }

    /// Where `cell`'s text is cut `amount` points into it: the first character starting at or
    /// past that point (the text's end when none does), and how far into the text it starts.
    pub(super) fn cut_at(&self, canvas: &Canvas<Paint>, palette: &Palette, text: &NSString, cell: &Range<usize>, amount: f64) -> (usize, f64) {
        let mut before = 0.0;
        for (run, paint) in canvas.runs_in(cell.clone()) {
            let piece = text.substringWithRange(ns(&run)).to_string();
            let font = palette.font_of(paint.look);
            let kern = palette.kern_of(paint.kern);
            let starts = self.char_starts(&piece, font);
            for &(units, width) in starts.iter() {
                let x = before + width + kern * units as f64;
                if x >= amount {
                    return (run.start + units, x);
                }
            }
            before += starts.last().map_or(0.0, |&(units, width)| width + kern * units as f64);
        }
        (cell.end, before)
    }

    /// Where each character of `piece` starts in `font`, and where it ends last: (UTF-16
    /// units in, points in), measured once per text and font.
    pub(super) fn char_starts(&self, piece: &str, font: &NSFont) -> CharStarts {
        let key = (piece.to_owned(), font as *const NSFont as usize);
        let mut cache = self.look().char_starts.borrow_mut();
        if cache.len() > MAX_PIECE_WIDTHS {
            cache.clear();
        }
        Rc::clone(cache.entry(key).or_insert_with(|| {
            let mut starts = Vec::with_capacity(piece.len() + 1);
            let mut units = 0;
            for (i, c) in piece.char_indices() {
                starts.push((units, if i == 0 { 0.0 } else { text_width(&NSString::from_str(&piece[..i]), font) }));
                units += c.len_utf16();
            }
            starts.push((units, text_width(&NSString::from_str(piece), font)));
            Rc::new(starts)
        }))
    }

    /// Whether an edit of `range` to `text` may go ahead as it is: anywhere outside tables, in a
    /// cell's text, or over whole tables (ADR 0021). Inside a cell a `|` is written `\|` and a
    /// line break a space; typed between cells it goes into the nearest one; deleting a hidden
    /// pipe or space, or across cells, does nothing. Those changes are made right after.
    pub(super) fn allow_table_edit(&self, view: &NSTextView, range: NSRange, text: &str) -> bool {
        let (start, end) = (range.location, range.location + range.length);
        let at_line_start = start == 0 || view.string().characterAtIndex(start - 1) == u16::from(b'\n');
        // What may be typed where is knotes-editor's to say (`table_typing`).
        let typing = knotes_editor::table_typing(&self.ivars().styled.borrow().tables, start, end, text, at_line_start, self.ivars().table_editing.get());
        match typing {
            TableTyping::Allow => true,
            TableTyping::AboveTable(line_break_at) => {
                if line_break_at.is_some() {
                    self.ivars().line_above_table.set(line_break_at);
                }
                true
            }
            TableTyping::Refuse => false,
            TableTyping::Instead { at, len, text } => {
                self.edit_later(view, NSRange::new(at, len), text);
                false
            }
        }
    }

    /// Replaces `range` with `text` once the edit being checked is over (changing the text from
    /// inside that check would confuse the text view).
    pub(super) fn edit_later(&self, view: &NSTextView, range: NSRange, text: String) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = dispatch2::MainThreadBound::new((self.retain(), view.retain()), mtm);
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            let (this, view) = target.get(mtm);
            this.ivars().table_editing.set(true);
            // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
            unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(&**view, &NSString::from_str(&text), range) };
            this.ivars().table_editing.set(false);
        });
    }

    /// Text was typed at the start of a table: a line break after it gives the table its line
    /// back, the cursor left before the break, so what is typed next goes on the new line. An
    /// edit of its own, at once (later, the next key would come first).
    pub(super) fn break_line_above_table(&self, view: &NSTextView) {
        let Some(at) = self.ivars().line_above_table.take() else { return };
        let (range, line_break) = (NSRange::new(at, 0), NSString::from_str("\n"));
        if view.shouldChangeTextInRange_replacementString(range, Some(&line_break)) {
            if let Some(storage) = unsafe { view.textStorage() } {
                storage.replaceCharactersInRange_withString(range, &line_break);
            }
            view.didChangeText();
            view.setSelectedRange(NSRange::new(at, 0));
        }
    }

    /// Tab, Shift+Tab, Return and arrows with the cursor in a table cell (ADR 0021): to the next
    /// or the previous cell, down a row, between rows and cells; Tab in the last cell and
    /// Return in the last row add a row. False when the command is not the table's.
    pub(super) fn table_command(&self, view: &NSTextView, command: Sel) -> bool {
        let selection = view.selectedRange();
        if selection.length != 0 {
            return false;
        }
        let at = selection.location;
        let styled = self.ivars().styled.borrow();
        let Some((t, row, column)) = cell_at(&styled.tables, at) else { return false };
        let table = &styled.tables[t];
        let rows = table.rows.len();
        let cell = &table.rows[row].cells[column];
        // A row may lack cells at its end: its last one stands in.
        let end_of = |r: usize, c: usize| table.rows.get(r).and_then(|row| row.cells.get(c).or(row.cells.last())).map(|c| c.end);
        let next = if column + 1 < table.rows[row].cells.len() { Some((row, column + 1)) } else { (row + 1 < rows).then_some((row + 1, 0)) };
        let previous = if column > 0 { Some((row, column - 1)) } else { row.checked_sub(1).map(|r| (r, table.rows[r].cells.len().saturating_sub(1))) };
        let target = if command == sel!(insertTab:) {
            match next {
                Some((r, c)) => end_of(r, c),
                None => {
                    drop(styled);
                    self.rewrite_table(view, t, |table| {
                        table.insert_row(table.rows.len());
                        (table.rows.len() - 1, 0)
                    });
                    return true;
                }
            }
        } else if command == sel!(insertBacktab:) {
            previous.and_then(|(r, c)| end_of(r, c)).or(Some(at))
        } else if command == sel!(insertNewline:) {
            if row + 1 < rows {
                end_of(row + 1, column)
            } else {
                drop(styled);
                self.rewrite_table(view, t, |table| {
                    table.insert_row(table.rows.len());
                    (table.rows.len() - 1, column)
                });
                return true;
            }
        } else if command == sel!(moveDown:) && row + 1 < rows {
            end_of(row + 1, column)
        } else if command == sel!(moveUp:) && row > 0 {
            end_of(row - 1, column)
        } else if command == sel!(moveRight:) && at == cell.end && next.is_some() {
            next.and_then(|(r, c)| table.rows[r].cells.get(c)).map(|c| c.start)
        } else if command == sel!(moveLeft:) && at == cell.start && previous.is_some() {
            previous.and_then(|(r, c)| end_of(r, c))
        } else {
            return false;
        };
        drop(styled);
        if let Some(target) = target {
            view.setSelectedRange(NSRange::new(target, 0));
            view.scrollRangeToVisible(NSRange::new(target, 0));
        }
        true
    }

    /// A cursor that landed between a table's cells (a click on a border or a cell's padding, an
    /// arrow from the line above) moves into the nearest cell.
    pub(super) fn keep_cursor_in_cells(&self, view: &NSTextView) {
        let selection = view.selectedRange();
        // Mid-edit the tables still have their old offsets: a key typed at the end of the line
        // above one put the cursor at its old start, and the next keys went into its first cell.
        if selection.length != 0 || self.ivars().pending.get().is_some() {
            return;
        }
        let at = selection.location;
        let edge = {
            let styled = self.ivars().styled.borrow();
            // A note starting with a table keeps its cursor at its very start, before the first
            // cell: the only place to type a line above it.
            if cell_at(&styled.tables, at).is_some() || (at == 0 && styled.tables.first().is_some_and(|t| t.range.start == 0)) { None } else { nearest_cell_edge(&styled.tables, at) }
        };
        if let Some(edge) = edge.filter(|&e| e != at) {
            view.setSelectedRange(NSRange::new(edge, 0));
        } else {
            self.show_cursor_column(view, at);
        }
    }

    /// The pointer moved to `point` (text view coordinates), or left the view: the table it is
    /// over, or near enough for its handles and add bars, shows them.
    pub fn hover_table(&self, view: &NSTextView, point: Option<NSPoint>) {
        let hover = point.and_then(|p| self.table_at_point(view, p));
        let old = self.ivars().hovered_table.replace(hover);
        if old == hover {
            return;
        }
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let ranges: Vec<(usize, usize)> = {
            let styled = self.ivars().styled.borrow();
            [old, hover]
                .into_iter()
                .flatten()
                .filter_map(|h| styled.tables.iter().find(|t| t.range.start == h.0))
                .map(|t| (t.range.start, t.range.end.min(storage.length())))
                .collect()
        };
        for range in ranges {
            self.restyle(&storage, range);
        }
    }

    /// The table at `point` (its start) with the row and column under it, the band around it
    /// where its handles and add bars are included. Rows are found by the fragment at the
    /// point's height, not by measuring every row (a long table's would take milliseconds).
    pub(super) fn table_at_point(&self, view: &NSTextView, point: NSPoint) -> Option<TableHover> {
        let layout = view.textLayoutManager()?;
        let content = layout.textContentManager()?;
        // A restyle (the pointer coming over the table restyles it) leaves rows to lay out.
        layout.textViewportLayoutController().layoutViewport();
        let origin = view.textContainerOrigin();
        let padding = layout.textContainer().map_or(0.0, |c| c.lineFragmentPadding());
        let styled = self.ivars().styled.borrow();
        let y = point.y - origin.y;
        let row_at = |y: f64| table_row_at(&layout, &content, &styled, y);
        // On a row; or just over the header (its column handles); or just under the last row
        // (the add row bar).
        let (t, row) = match row_at(y) {
            Some((t, r)) => (t, Some(r)),
            None => match row_at(y + TABLE_NEAR_TOP).filter(|&(_, r)| r == 0) {
                Some((t, _)) => (t, None),
                None => {
                    let (t, r) = row_at(y - TABLE_NEAR_SIDE)?;
                    (r + 1 == styled.tables[t].rows.len()).then_some((t, None))?
                }
            },
        };
        let table = &styled.tables[t];
        let edges = self.ivars().table_columns.borrow().get(&table.range.start).cloned()?;
        let x0 = origin.x + padding;
        let (left, right) = ((x0 + edges.first()?).max(x0), (x0 + edges.last()?).min(x0 + text_column(view)));
        if point.x < left - TABLE_NEAR_SIDE || point.x > right + TABLE_NEAR_SIDE {
            return None;
        }
        let column = edges.windows(2).position(|w| point.x >= x0 + w[0] && point.x < x0 + w[1]);
        Some((table.range.start, row, column))
    }

    /// Sideways scrolling (`dx` points, as the wheel or trackpad gives it, momentum too) with
    /// the pointer at `point`: over a table wider than the text column, the table follows it.
    /// False when no such table is there.
    pub fn scroll_table(&self, view: &NSTextView, point: NSPoint, dx: f64) -> bool {
        let Some((start, _, _)) = self.table_at_point(view, point) else { return false };
        let Some(t) = self.ivars().styled.borrow().tables.iter().position(|t| t.range.start == start) else { return false };
        let Some(widths) = self.ivars().table_widths.borrow().get(&t).cloned() else { return false };
        let most = (widths.iter().sum::<f64>() - text_column(view)).max(0.0);
        if most <= 0.0 {
            return false;
        }
        let scroll = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0.0);
        let next = (scroll - dx).clamp(0.0, most);
        if (next - scroll).abs() > 0.01 {
            // Only the rows in view now (a long table's every row would take tens of
            // milliseconds a step); the others once the scrolling stops.
            self.ivars().apply_within.set(viewport_range(view));
            self.scroll_table_to(view, t, next);
            self.ivars().apply_within.set(None);
            self.catch_up_later(view, t);
        }
        true
    }

    /// Sets table `t`'s rows out of view a moment after its last sideways scroll step.
    pub(super) fn catch_up_later(&self, view: &NSTextView, t: usize) {
        self.ivars().stale_table.set(Some(t));
        let generation = self.ivars().catch_up.get() + 1;
        self.ivars().catch_up.set(generation);
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = dispatch2::MainThreadBound::new((self.retain(), view.retain()), mtm);
        let when = dispatch2::DispatchTime::try_from(std::time::Duration::from_millis(CATCH_UP_MS)).expect("short delay");
        let _ = DispatchQueue::main().after(when, move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            let (this, view) = target.get(mtm);
            if this.ivars().catch_up.get() == generation {
                this.catch_up_table(view);
            }
        });
    }

    /// The table whose rows out of view wait for its sideways scroll: all set now.
    pub(super) fn catch_up_table(&self, view: &NSTextView) {
        if let Some(t) = self.ivars().stale_table.take() {
            let scroll = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0.0);
            self.scroll_table_to(view, t, scroll);
        }
    }

    pub(super) fn scroll_table_to(&self, view: &NSTextView, t: usize, scroll: f64) {
        self.ivars().table_scroll.borrow_mut().insert(t, scroll);
        let range = self.ivars().styled.borrow().tables.get(t).map(|t| (t.range.start, t.range.end));
        if let (Some(storage), Some(range)) = (unsafe { view.textStorage() }, range) {
            self.restyle(&storage, (range.0, range.1.min(storage.length())));
        }
    }

    /// The cursor in a cell of a column scrolled out of view, or partly: the table scrolls to
    /// show the column.
    pub(super) fn show_cursor_column(&self, view: &NSTextView, at: usize) {
        let Some((t, _, column)) = cell_at(&self.ivars().styled.borrow().tables, at) else { return };
        let Some(widths) = self.ivars().table_widths.borrow().get(&t).cloned() else { return };
        let scroll = self.ivars().table_scroll.borrow().get(&t).copied().unwrap_or(0.0);
        let room = text_column(view);
        let left: f64 = widths[..column.min(widths.len())].iter().sum();
        let right = left + widths.get(column).copied().unwrap_or(0.0);
        let next = if left < scroll {
            left
        } else if right > scroll + room {
            (right - room).min(left)
        } else {
            scroll
        };
        if (next - scroll).abs() > 0.01 {
            self.scroll_table_to(view, t, next);
        }
    }

    /// The control of the table under the pointer at `point` (text view coordinates).
    pub fn table_control_at(&self, view: &NSTextView, point: NSPoint) -> Option<TableControl> {
        let (start, row, column) = self.ivars().hovered_table.get()?;
        let layout = view.textLayoutManager()?;
        let content = layout.textContentManager()?;
        let origin = view.textContainerOrigin();
        let padding = layout.textContainer().map_or(0.0, |c| c.lineFragmentPadding());
        let styled = self.ivars().styled.borrow();
        let t = styled.tables.iter().position(|t| t.range.start == start)?;
        let table = &styled.tables[t];
        let edges = self.ivars().table_columns.borrow().get(&start).cloned()?;
        let last = table.rows.len().saturating_sub(1);
        let (left, width) = (origin.x + padding + edges.first()?, edges.last()? - edges.first()?);
        // As drawn: the row's handle and the add row bar along the part within the text column,
        // no add column bar while the table's right edge is cut.
        let (column_left, column_right) = (origin.x + padding, origin.x + padding + text_column(view));
        let (shown_left, shown_right) = (left.max(column_left), (left + width).min(column_right));
        let cut = left + width > column_right + 0.5;
        layout.textViewportLayoutController().layoutViewport();
        // Only the rows with controls: the pointer's (its handle, its part of the add column
        // bar), the header (column handles), the last (the add row bar).
        let pointed = table_row_at(&layout, &content, &styled, point.y - origin.y).filter(|&(at, _)| at == t).map(|(_, r)| r);
        let mut wanted: Vec<usize> = [row, Some(0), Some(last), pointed].into_iter().flatten().collect();
        wanted.sort_unstable();
        wanted.dedup();
        for i in wanted {
            let r = &table.rows[i];
            let Some((top, bottom)) = row_band(&layout, &content, r.range.start) else { continue };
            let rect = NSRect::new(NSPoint::new(left, origin.y + top), NSSize::new(width, bottom - top));
            let shown = NSRect::new(NSPoint::new(shown_left, origin.y + top), NSSize::new(shown_right - shown_left, bottom - top));
            let controls = TableControls { handle: row == Some(i), column_handle: if i == 0 { column } else { None }, add_row: i == last, add_column: !cut, ..TableControls::default() };
            for (kind, c) in table_controls(rect, shown, &edges, &controls) {
                let inside = point.x >= c.origin.x && point.x <= c.origin.x + c.size.width && point.y >= c.origin.y && point.y <= c.origin.y + c.size.height;
                if inside {
                    return Some(match kind {
                        "table-row-handle" => TableControl::RowHandle(t, i),
                        "table-column-handle" => TableControl::ColumnHandle(t, column?),
                        "table-add-row" => TableControl::AddRow(t),
                        _ => TableControl::AddColumn(t),
                    });
                }
            }
        }
        None
    }

    /// A mouse press at `point`: an add bar adds at once; a handle followed by the mouse is a
    /// drag that moves its row or column, or else a click that opens its menu. False when no
    /// control is there.
    pub fn press_table(&self, view: &NSTextView, point: NSPoint) -> bool {
        let Some(control) = self.table_control_at(view, point) else { return false };
        if self.add_from(view, control) {
            return true;
        }
        let Some(window) = view.window() else { return true };
        let mut dragged = None;
        while let Some(event) = window.nextEventMatchingMask(objc2_app_kit::NSEventMask::LeftMouseDragged | objc2_app_kit::NSEventMask::LeftMouseUp) {
            let at = view.convertPoint_fromView(event.locationInWindow(), None);
            if dragged.is_some() || (at.x - point.x).hypot(at.y - point.y) > 4.0 {
                dragged = Some(at);
            }
            if event.r#type() == objc2_app_kit::NSEventType::LeftMouseUp {
                break;
            }
        }
        match dragged {
            Some(to) => self.drop_handle(view, control, to),
            None => self.show_table_menu(view, control, point),
        }
        true
    }

    /// `KNOTES_PROBE_TABLE_PRESS`: a press at `x,y` (a handle's menu printed, not shown),
    /// `x,y:n` (its item n chosen) or `x,y>x,y` (a handle dragged there).
    pub fn probe_table_press(&self, view: &NSTextView, spec: &str) {
        let point = |p: &str| p.split_once(',').and_then(|(x, y)| Some(NSPoint::new(x.parse().ok()?, y.parse().ok()?)));
        let (at, rest) = spec.split_once([':', '>']).map_or((spec, None), |(a, r)| (a, Some(r)));
        let (Some(at), Some(control)) = (point(at), point(at).and_then(|p| self.table_control_at(view, p))) else {
            println!("table-control: none");
            return;
        };
        let _ = at;
        if self.add_from(view, control) {
            return;
        }
        if spec.contains('>') {
            if let Some(to) = rest.and_then(point) {
                self.drop_handle(view, control, to);
            }
        } else if let Some(n) = rest.and_then(|r| r.parse::<usize>().ok()) {
            if let Some(&(_, action)) = self.table_menu(control).get(n) {
                self.act_on_table(view, control_table(control), action);
            }
        } else {
            let titles: Vec<&str> = self.table_menu(control).iter().map(|(title, _)| *title).collect();
            println!("table-menu: {}", titles.join(" | "));
        }
    }

    /// An add bar's row or column, added; false for a handle.
    pub(super) fn add_from(&self, view: &NSTextView, control: TableControl) -> bool {
        let size = |t: usize| self.ivars().styled.borrow().tables.get(t).map(|t| (t.rows.len(), t.aligns.len()));
        match control {
            TableControl::AddRow(t) => {
                if let Some((rows, _)) = size(t) {
                    self.act_on_table(view, t, TableAction::InsertRow(rows));
                }
                true
            }
            TableControl::AddColumn(t) => {
                if let Some((_, columns)) = size(t) {
                    self.act_on_table(view, t, TableAction::InsertColumn(columns));
                }
                true
            }
            _ => false,
        }
    }

    /// A handle's menu: its titles and actions.
    pub(super) fn table_menu(&self, control: TableControl) -> Vec<(&'static str, TableAction)> {
        match control {
            TableControl::RowHandle(_, row) => vec![
                (strings::TABLE_ROW_ABOVE, TableAction::InsertRow(row)),
                (strings::TABLE_ROW_BELOW, TableAction::InsertRow(row + 1)),
                (strings::TABLE_ROW_DELETE, TableAction::DeleteRow(row)),
            ],
            TableControl::ColumnHandle(_, c) => vec![
                (strings::TABLE_COLUMN_LEFT, TableAction::InsertColumn(c)),
                (strings::TABLE_COLUMN_RIGHT, TableAction::InsertColumn(c + 1)),
                (strings::TABLE_COLUMN_DELETE, TableAction::DeleteColumn(c)),
                (strings::TABLE_ALIGN_LEFT, TableAction::Align(c, Align::Left)),
                (strings::TABLE_ALIGN_CENTER, TableAction::Align(c, Align::Center)),
                (strings::TABLE_ALIGN_RIGHT, TableAction::Align(c, Align::Right)),
                (strings::TABLE_SORT_UP, TableAction::Sort(c, true)),
                (strings::TABLE_SORT_DOWN, TableAction::Sort(c, false)),
            ],
            TableControl::AddRow(_) | TableControl::AddColumn(_) => Vec::new(),
        }
    }

    /// Shows a handle's menu at `point`; the chosen item acts (`tableMenuItem:`).
    pub(super) fn show_table_menu(&self, view: &NSTextView, control: TableControl, point: NSPoint) {
        let menu = objc2_app_kit::NSMenu::new(self.mtm());
        self.fill_table_menu(&menu, control_table(control), &self.table_menu(control), 0);
        menu.popUpMenuPositioningItem_atLocation_inView(None, point, Some(view));
    }

    /// Puts table `t`'s `items` in `menu` from position `at`, groups apart (rows; columns;
    /// alignment; sorting); the chosen item acts (`tableMenuItem:`). Gives the position after
    /// them.
    pub(super) fn fill_table_menu(&self, menu: &objc2_app_kit::NSMenu, t: usize, items: &[(&'static str, TableAction)], mut at: isize) -> isize {
        let mtm = self.mtm();
        *self.ivars().menu_actions.borrow_mut() = items.iter().map(|(_, a)| (t, *a)).collect();
        let group = |a: &TableAction| match a {
            TableAction::InsertRow(_) | TableAction::DeleteRow(_) | TableAction::MoveRow(..) => 0,
            TableAction::InsertColumn(_) | TableAction::DeleteColumn(_) | TableAction::MoveColumn(..) => 1,
            TableAction::Align(..) => 2,
            TableAction::Sort(..) => 3,
        };
        for (tag, (title, action)) in items.iter().enumerate() {
            if tag > 0 && group(&items[tag - 1].1) != group(action) {
                menu.insertItem_atIndex(&objc2_app_kit::NSMenuItem::separatorItem(mtm), at);
                at += 1;
            }
            // SAFETY: the live preview is the target and lives as long as the view.
            let item = unsafe { objc2_app_kit::NSMenuItem::initWithTitle_action_keyEquivalent(objc2_app_kit::NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(sel!(tableMenuItem:)), &NSString::new()) };
            // SAFETY: as above.
            unsafe { item.setTarget(Some(self)) };
            item.setTag(tag as isize);
            menu.insertItem_atIndex(&item, at);
            at += 1;
        }
        at
    }

    /// The table actions for the cell holding character `index` (or nearest to it on a
    /// table's line): its row's, then its column's, with the table's index.
    pub(super) fn cell_menu(&self, index: usize) -> Option<(usize, Vec<(&'static str, TableAction)>)> {
        let cell = {
            let styled = self.ivars().styled.borrow();
            cell_at(&styled.tables, index).or_else(|| nearest_cell_edge(&styled.tables, index).and_then(|edge| cell_at(&styled.tables, edge)))
        };
        let (t, row, column) = cell?;
        let mut items = self.table_menu(TableControl::RowHandle(t, row));
        items.extend(self.table_menu(TableControl::ColumnHandle(t, column)));
        Some((t, items))
    }

    /// The table actions a right click at `point` offers: a handle's (only its row's or
    /// column's), or a cell's (its row's and column's, the nearest cell on a table's line), with
    /// the table's index and whether they stand alone (a handle's) or come before the text's.
    pub(super) fn context_items(&self, view: &NSTextView, point: NSPoint) -> Option<ContextItems> {
        match self.table_control_at(view, point) {
            Some(control @ (TableControl::RowHandle(..) | TableControl::ColumnHandle(..))) => Some((control_table(control), self.table_menu(control), true)),
            Some(_) => None,
            None => self.cell_menu(view.characterIndexForInsertionAtPoint(point)).map(|(t, items)| (t, items, false)),
        }
    }

    /// The context menu for a right click at `point`: a handle's menu alone, or the text's
    /// `menu` with a cell's row and column actions first.
    pub fn table_context_menu(&self, view: &NSTextView, point: NSPoint, menu: Option<Retained<objc2_app_kit::NSMenu>>) -> Option<Retained<objc2_app_kit::NSMenu>> {
        let Some((t, items, alone)) = self.context_items(view, point) else { return menu };
        let menu = match menu {
            Some(menu) if !alone => menu,
            _ => objc2_app_kit::NSMenu::new(self.mtm()),
        };
        let at = self.fill_table_menu(&menu, t, &items, 0);
        if menu.numberOfItems() > at {
            menu.insertItem_atIndex(&objc2_app_kit::NSMenuItem::separatorItem(self.mtm()), at);
        }
        Some(menu)
    }

    /// `KNOTES_PROBE_TABLE_CONTEXT_AT=x,y`: the table items a right click there offers.
    pub fn probe_table_context_at(&self, view: &NSTextView, spec: &str) {
        let point = spec.split_once(',').and_then(|(x, y)| Some(NSPoint::new(x.parse().ok()?, y.parse().ok()?)));
        match point.and_then(|p| self.context_items(view, p)) {
            Some((_, items, _)) => println!("table-context: {}", items.iter().map(|(title, _)| *title).collect::<Vec<_>>().join(" | ")),
            None => println!("table-context: none"),
        }
    }

    /// `KNOTES_PROBE_TABLE_CONTEXT`: a right click at character `index` (its table items
    /// printed, `none` outside a table), or `index:n` (item n chosen).
    pub fn probe_table_context(&self, view: &NSTextView, spec: &str) {
        let (index, choice) = spec.split_once(':').map_or((spec, None), |(i, n)| (i, n.parse::<usize>().ok()));
        let Some((t, items)) = index.parse().ok().and_then(|i| self.cell_menu(i)) else {
            println!("table-context: none");
            return;
        };
        match choice.and_then(|n| items.get(n)) {
            Some(&(_, action)) => self.act_on_table(view, t, action),
            None => println!("table-context: {}", items.iter().map(|(title, _)| *title).collect::<Vec<_>>().join(" | ")),
        }
    }

    /// A handle dropped at `point`: its row or column moves to the one there.
    pub(super) fn drop_handle(&self, view: &NSTextView, control: TableControl, point: NSPoint) {
        let action = {
            let Some(layout) = view.textLayoutManager() else { return };
            let Some(content) = layout.textContentManager() else { return };
            let origin = view.textContainerOrigin();
            let styled = self.ivars().styled.borrow();
            match control {
                TableControl::RowHandle(t, from) => {
                    let Some(table) = styled.tables.get(t) else { return };
                    layout.textViewportLayoutController().layoutViewport();
                    let y = point.y - origin.y;
                    let header_top = table.rows.first().and_then(|r| row_band(&layout, &content, r.range.start)).map_or(0.0, |b| b.0);
                    let to = match table_row_at(&layout, &content, &styled, y) {
                        Some((at, r)) if at == t => r,
                        _ if y < header_top => 0,
                        _ => table.rows.len().saturating_sub(1),
                    };
                    TableAction::MoveRow(from, to)
                }
                TableControl::ColumnHandle(t, from) => {
                    let Some(table) = styled.tables.get(t) else { return };
                    let Some(edges) = self.ivars().table_columns.borrow().get(&table.range.start).cloned() else { return };
                    let padding = layout.textContainer().map_or(0.0, |c| c.lineFragmentPadding());
                    let x = point.x - origin.x - padding;
                    let to = edges.windows(2).position(|w| x >= w[0] && x < w[1]).unwrap_or(if x < edges[0] { 0 } else { edges.len().saturating_sub(2) });
                    TableAction::MoveColumn(from, to)
                }
                _ => return,
            }
        };
        if !matches!(action, TableAction::MoveRow(a, b) | TableAction::MoveColumn(a, b) if a == b) {
            self.act_on_table(view, control_table(control), action);
        }
    }

    /// Does `action` to table `t`, as one edit, the cursor left in the cell it concerns.
    pub(super) fn act_on_table(&self, view: &NSTextView, t: usize, action: TableAction) {
        self.rewrite_table(view, t, |table| match action {
            TableAction::InsertRow(at) => {
                table.insert_row(at);
                (at.min(table.rows.len() - 1), 0)
            }
            TableAction::DeleteRow(row) => {
                table.delete_row(row);
                (row.min(table.rows.len() - 1), 0)
            }
            TableAction::InsertColumn(at) => {
                table.insert_column(at);
                (0, at.min(table.columns() - 1))
            }
            TableAction::DeleteColumn(column) => {
                table.delete_column(column);
                (0, column.min(table.columns() - 1))
            }
            TableAction::MoveRow(from, to) => {
                table.move_row(from, to);
                (to, 0)
            }
            TableAction::MoveColumn(from, to) => {
                table.move_column(from, to);
                (0, to)
            }
            TableAction::Align(column, align) => {
                table.set_align(column, align);
                (0, column)
            }
            TableAction::Sort(column, ascending) => {
                table.sort_by(column, ascending);
                (0, column)
            }
        });
    }

    /// Rewrites table `t` (its index in the note) as `change` makes it, as one edit, and puts
    /// the cursor at the end of the cell (row, column) `change` gives.
    pub(super) fn rewrite_table(&self, view: &NSTextView, t: usize, change: impl FnOnce(&mut TableText) -> (usize, usize)) {
        let Some(storage) = (unsafe { view.textStorage() }) else { return };
        let Some(range) = self.ivars().styled.borrow().tables.get(t).map(|t| t.range.clone()) else { return };
        let source = storage.string().substringWithRange(ns(&range)).to_string();
        let Some(mut table) = TableText::parse(&source) else { return };
        let (row, column) = change(&mut table);
        let mut new = table.write();
        // At the note's end the table may have no final line break.
        if !source.ends_with('\n') {
            new.pop();
        }
        self.ivars().table_editing.set(true);
        // SAFETY: an NSString is a valid argument for insertText:replacementRange:.
        unsafe { objc2_app_kit::NSTextInputClient::insertText_replacementRange(view, &NSString::from_str(&new), ns(&range)) };
        self.ivars().table_editing.set(false);
        let end = table.cell_offset(row, column) + table.rows[row][column].len();
        let at = range.start + new[..end.min(new.len())].encode_utf16().count();
        view.setSelectedRange(NSRange::new(at, 0));
    }
}

/// The table a control belongs to.
pub(super) fn control_table(control: TableControl) -> usize {
    match control {
        TableControl::RowHandle(t, _) | TableControl::ColumnHandle(t, _) | TableControl::AddRow(t) | TableControl::AddColumn(t) => t,
    }
}

/// The top and bottom of the row starting at `offset` (text container coordinates), without
/// the empty line after the note's final line break.
pub(super) fn row_band(layout: &NSTextLayoutManager, content: &objc2_app_kit::NSTextContentManager, offset: usize) -> Option<(f64, f64)> {
    let location = content.locationFromLocation_withOffset(&content.documentRange().location(), offset as isize)?;
    let fragment = layout.textLayoutFragmentForLocation(&location)?;
    let top = fragment.layoutFragmentFrame().origin.y;
    Some((top, top + crate::preview_fragment::row_height(&fragment)))
}

/// The table (its index) and row whose line is at height `y` (text container coordinates).
pub(super) fn table_row_at(layout: &NSTextLayoutManager, content: &objc2_app_kit::NSTextContentManager, styled: &Highlight, y: f64) -> Option<(usize, usize)> {
    let fragment = layout.textLayoutFragmentForPosition(NSPoint::new(1.0, y))?;
    let start = content.offsetFromLocation_toLocation(&content.documentRange().location(), &fragment.rangeInElement().location()).max(0) as usize;
    let t = styled.tables.iter().position(|t| t.range.start <= start && start < t.range.end)?;
    let row = styled.tables[t].rows.iter().position(|r| r.range.start == start)?;
    // Not past the row's own lines (the note's last row also holds the empty line after it).
    let (top, bottom) = row_band(layout, content, start)?;
    (y >= top && y < bottom).then_some((t, row))
}
