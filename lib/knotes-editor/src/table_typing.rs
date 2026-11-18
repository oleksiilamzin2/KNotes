//! What may be typed where in a note's tables (ADR 0005): a table is edited through its cells,
//! so its pipes and its delimiter row are never typed over, and nothing typed breaks it into
//! plain text. Places are in the text view's units, as the tables' ranges are.

use crate::highlight::TableRun;

/// What becomes of an edit that touches a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableTyping {
    /// It is made as typed.
    Allow,
    /// It is made as typed, before the table's start; when a place is given, a line break is
    /// owed there, to give the table its line back.
    AboveTable(Option<usize>),
    /// It is not made.
    Refuse,
    /// It is not made; this is, once the edit being checked is over: `text` in the place of
    /// `len` units at `at`.
    Instead { at: usize, len: usize, text: String },
}

/// The table, row and cell whose text holds `index` (its end too).
pub fn cell_at(tables: &[TableRun], index: usize) -> Option<(usize, usize, usize)> {
    tables.iter().enumerate().filter(|(_, t)| t.range.start <= index && index <= t.range.end).find_map(|(t, table)| {
        table.rows.iter().enumerate().find_map(|(r, row)| row.cells.iter().position(|c| c.start <= index && index <= c.end).map(|c| (t, r, c)))
    })
}

/// The nearest cell edge to `index` when it lies on a table's line outside its cells (on a
/// hidden pipe or space, or the folded delimiter row).
pub fn nearest_cell_edge(tables: &[TableRun], index: usize) -> Option<usize> {
    let table = tables.iter().find(|t| t.range.start <= index && index < t.range.end)?;
    if table.delimiter.start <= index && index <= table.delimiter.end {
        return table.rows.first()?.cells.last().map(|c| c.end);
    }
    let row = table.rows.iter().find(|r| r.range.start <= index && index <= r.range.end)?;
    row.cells.iter().flat_map(|c| [c.start, c.end]).min_by_key(|p| p.abs_diff(index))
}

/// `text` fit for a table cell: a `|` escaped, line breaks as spaces.
pub fn cell_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut escaped = false;
    for c in text.chars() {
        match c {
            '|' if !escaped => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
        escaped = c == '\\' && !escaped;
    }
    out
}

/// What becomes of `text` typed in the place of `start..end`. `at_line_start`: `start` begins a
/// line; `own_edit`: the edit is the editor's own (a table written again, `Instead` made),
/// which is made as it is.
pub fn table_typing(tables: &[TableRun], start: usize, end: usize, text: &str, at_line_start: bool, own_edit: bool) -> TableTyping {
    let allowed = |allow: bool| if allow { TableTyping::Allow } else { TableTyping::Refuse };
    // Above a table: what is removed up to its start must leave it starting a line, or the
    // line above joins its header (`Intro| a | b |`) and it is a table no more.
    if end > start && tables.iter().any(|t| t.range.start == end) {
        return allowed(at_line_start || text.ends_with('\n'));
    }
    // Typed at a table's start (the cursor may stand there, before its first cell): on a line
    // of its own above it, so that a note starting with a table can get a line before it (it
    // went into the first cell).
    if end == start && !own_edit && tables.iter().any(|t| t.range.start == start) {
        return TableTyping::AboveTable((!text.ends_with('\n')).then(|| start + text.encode_utf16().count()));
    }
    let Some(table) = tables.iter().find(|t| start < t.range.end && (end > t.range.start || (start == end && start >= t.range.start))) else {
        return TableTyping::Allow;
    };
    // The whole table replaced or removed.
    if start <= table.range.start && end >= table.range.end {
        return TableTyping::Allow;
    }
    let clean = cell_text(text);
    // Inside one cell: as typed, unless it would break the row (a pipe, a line break).
    if table.rows.iter().flat_map(|r| r.cells.iter()).any(|c| c.start <= start && end <= c.end) {
        return if clean == text { TableTyping::Allow } else { TableTyping::Instead { at: start, len: end - start, text: clean } };
    }
    // On a pipe, a space beside one or the delimiter row: what is typed goes to the nearest
    // cell; nothing there is removed.
    match nearest_cell_edge(tables, start).filter(|_| end == start && !text.is_empty()) {
        Some(at) => TableTyping::Instead { at, len: 0, text: clean },
        None => TableTyping::Refuse,
    }
}
