//! A Markdown table's cells, changed as a whole and written back (ADR 0021): rows and columns
//! added, removed or moved, alignment, sorting. The table is written evenly, `| a | b |`, one
//! space inside each pipe.

use crate::highlight::{Align, highlight};

/// A table's cells as written (Markdown, `\|` kept), the header row first, every row as long
/// as the table is wide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableText {
    pub aligns: Vec<Align>,
    pub rows: Vec<Vec<String>>,
}

impl TableText {
    /// The table `text` starts with.
    pub fn parse(text: &str) -> Option<Self> {
        let table = highlight(text).tables.into_iter().next()?;
        let columns = table.aligns.len();
        let rows = table
            .rows
            .iter()
            .map(|row| {
                let mut cells: Vec<String> = row.cells.iter().map(|c| text[c.clone()].to_owned()).collect();
                cells.resize(columns.max(cells.len()), String::new());
                cells
            })
            .collect();
        Some(Self { aligns: table.aligns, rows })
    }

    pub fn columns(&self) -> usize {
        self.aligns.len()
    }

    /// The table's Markdown, each line ending with a line break.
    pub fn write(&self) -> String {
        let line = |cells: &[String]| format!("| {} |\n", cells.join(" | "));
        let delimiter: Vec<String> = self
            .aligns
            .iter()
            .map(|a| match a {
                Align::None => "---",
                Align::Left => ":---",
                Align::Center => ":---:",
                Align::Right => "---:",
            })
            .map(str::to_owned)
            .collect();
        let mut out = String::new();
        for (i, row) in self.rows.iter().enumerate() {
            out += &line(row);
            if i == 0 {
                out += &line(&delimiter);
            }
        }
        out
    }

    /// Where the text of cell `column` of row `row` (the header is row 0) starts in `write()`.
    pub fn cell_offset(&self, row: usize, column: usize) -> usize {
        let line = |cells: &[String]| cells.iter().map(|c| c.len() + 3).sum::<usize>() + 2;
        let delimiter: usize = self.aligns.iter().map(|a| if matches!(a, Align::Center) { 5 } else if matches!(a, Align::None) { 3 } else { 4 } + 3).sum::<usize>() + 2;
        let before: usize = self.rows[..row].iter().map(|r| line(r)).sum::<usize>() + if row > 0 { delimiter } else { 0 };
        before + 2 + self.rows[row][..column].iter().map(|c| c.len() + 3).sum::<usize>()
    }

    /// An empty row before row `at` (the header is row 0; `rows.len()` adds one at the end).
    pub fn insert_row(&mut self, at: usize) {
        self.rows.insert(at.min(self.rows.len()), vec![String::new(); self.columns()]);
    }

    /// Takes row `at` out; the header's going makes the next row the header. The last row
    /// stays.
    pub fn delete_row(&mut self, at: usize) {
        if self.rows.len() > 1 && at < self.rows.len() {
            self.rows.remove(at);
        }
    }

    /// An empty column before column `at` (`columns()` adds one at the end).
    pub fn insert_column(&mut self, at: usize) {
        let at = at.min(self.columns());
        self.aligns.insert(at, Align::None);
        for row in &mut self.rows {
            row.insert(at, String::new());
        }
    }

    /// Takes column `at` out. The last column stays.
    pub fn delete_column(&mut self, at: usize) {
        if self.columns() > 1 && at < self.columns() {
            self.aligns.remove(at);
            for row in &mut self.rows {
                row.remove(at);
            }
        }
    }

    /// Moves row `from` to be row `to`.
    pub fn move_row(&mut self, from: usize, to: usize) {
        if from < self.rows.len() && to < self.rows.len() {
            let row = self.rows.remove(from);
            self.rows.insert(to, row);
        }
    }

    /// Moves column `from` to be column `to`, with its alignment.
    pub fn move_column(&mut self, from: usize, to: usize) {
        if from < self.columns() && to < self.columns() {
            let align = self.aligns.remove(from);
            self.aligns.insert(to, align);
            for row in &mut self.rows {
                let cell = row.remove(from);
                row.insert(to, cell);
            }
        }
    }

    pub fn set_align(&mut self, column: usize, align: Align) {
        if let Some(a) = self.aligns.get_mut(column) {
            *a = align;
        }
    }

    /// Sorts the rows under the header by column `column`: numbers as numbers (before text),
    /// text ignoring case; equal rows keep their order.
    pub fn sort_by(&mut self, column: usize, ascending: bool) {
        let key = |row: &Vec<String>| {
            let cell = row.get(column).map_or("", |c| c.trim());
            (cell.replace(',', ".").parse::<f64>().ok(), cell.to_lowercase())
        };
        if self.rows.len() > 2 {
            self.rows[1..].sort_by(|a, b| {
                let (a, b) = (key(a), key(b));
                let order = match (a.0, b.0) {
                    (Some(x), Some(y)) => x.total_cmp(&y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.1.cmp(&b.1),
                };
                if ascending { order } else { order.reverse() }
            });
        }
    }
}
