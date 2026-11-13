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
}
