use knotes_editor::{TableTyping, cell_at, cell_text, highlight, nearest_cell_edge, table_typing};

/// A line, a table of two rows, an empty line (a line right under a table is its row), a line. In ASCII, so a byte is a unit of the text view.
const NOTE: &str = "Intro\n| a | bb |\n|---|---|\n| c | dd |\n\nAfter\n";

fn at(piece: &str) -> usize {
    NOTE.find(piece).unwrap_or_else(|| panic!("{piece}"))
}

#[test]
fn a_cell_is_found_by_a_place_in_it_and_a_pipe_by_the_cell_beside_it() {
    let h = highlight(NOTE);
    let (a, bb, dd) = (at("a |"), at("bb"), at("dd"));
    assert_eq!((cell_at(&h.tables, a), cell_at(&h.tables, a + 1), cell_at(&h.tables, bb + 1), cell_at(&h.tables, dd + 2)), (Some((0, 0, 0)), Some((0, 0, 0)), Some((0, 0, 1)), Some((0, 1, 1))));
    assert_eq!((cell_at(&h.tables, 0), cell_at(&h.tables, at("| a")), cell_at(&h.tables, at("After"))), (None, None, None), "the text around, and a pipe");
    // On the pipe before a cell, the cell's start; on the delimiter row, the header's last end.
    assert_eq!(nearest_cell_edge(&h.tables, at("| a")), Some(a));
    assert_eq!(nearest_cell_edge(&h.tables, at("|---") + 2), Some(bb + 2));
    assert_eq!((nearest_cell_edge(&h.tables, 2), nearest_cell_edge(&h.tables, at("After") + 1)), (None, None));
    assert_eq!((cell_text("a|b"), cell_text("a\\|b"), cell_text("one\ntwo\r"), cell_text("plain")), ("a\\|b".to_owned(), "a\\|b".to_owned(), "one two ".to_owned(), "plain".to_owned()));
}

#[test]
fn a_table_is_typed_in_through_its_cells_and_never_broken() {
    let h = highlight(NOTE);
    let typing = |start: usize, end: usize, text: &str| table_typing(&h.tables, start, end, text, start == 0 || NOTE.as_bytes()[start - 1] == b'\n', false);
    let (table, a, bb) = (at("| a"), at("a |"), at("bb"));
    // Outside the table: as typed.
    assert_eq!((typing(2, 2, "x"), typing(at("After"), at("After") + 2, "")), (TableTyping::Allow, TableTyping::Allow));
    // In a cell: as typed; a pipe or a line break would break the row, so it goes in tamed.
    assert_eq!(typing(a + 1, a + 1, "x"), TableTyping::Allow);
    assert_eq!(typing(bb, bb + 2, "new"), TableTyping::Allow, "a cell's text replaced");
    assert_eq!(typing(a + 1, a + 1, "x|y"), TableTyping::Instead { at: a + 1, len: 0, text: "x\\|y".into() });
    assert_eq!(typing(bb, bb + 2, "one\ntwo"), TableTyping::Instead { at: bb, len: 2, text: "one two".into() });
    // On a pipe or the delimiter row: typed, it goes to the nearest cell; removed, nothing is.
    assert_eq!(typing(table + 1, table + 1, "x"), TableTyping::Instead { at: a, len: 0, text: "x".into() });
    assert_eq!(typing(at("|---") + 2, at("|---") + 2, "x"), TableTyping::Instead { at: bb + 2, len: 0, text: "x".into() });
    assert_eq!(typing(table, table + 1, ""), TableTyping::Refuse, "the first pipe is not deleted");
    assert_eq!(typing(a, bb + 1, ""), TableTyping::Refuse, "nor a selection over two cells");
    assert_eq!(typing(at("|---"), at("|---") + 3, ""), TableTyping::Refuse);
    // The whole table removed or replaced: as typed.
    assert_eq!(typing(table, at("After"), ""), TableTyping::Allow);
    assert_eq!(typing(0, NOTE.len(), "all new"), TableTyping::Allow);
    // The line above joined to it would make it text: the line break before it stays unless
    // what is left still starts a line, or what is typed ends one.
    assert_eq!(typing(table - 1, table, ""), TableTyping::Refuse, "«Intro| a | bb |» is no table");
    assert_eq!(typing(0, table, ""), TableTyping::Allow, "the whole line above removed");
    assert_eq!(typing(table - 1, table, "!\n"), TableTyping::Allow);
    // Typed at its very start: above it, a line break owed after what was typed; one that ends
    // a line owes none. The editor's own edit there is made as it is.
    assert_eq!(typing(table, table, "New"), TableTyping::AboveTable(Some(table + 3)));
    assert_eq!(typing(table, table, "New\n"), TableTyping::AboveTable(None));
    assert_eq!(table_typing(&h.tables, table, table, "x", true, true), TableTyping::Instead { at: a, len: 0, text: "x".into() });
    // No table, nothing to keep.
    assert_eq!(table_typing(&[], 3, 5, "x|y\n", false, false), TableTyping::Allow);
}
