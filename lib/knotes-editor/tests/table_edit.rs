use knotes_editor::{Align, TableText};

const SRC: &str = "| Name | Age |\n|:---|---:|\n| Ann | 3 |\n| Bob |\n";

#[test]
fn a_table_reads_its_cells_and_writes_them_back_evenly() {
    let t = TableText::parse(SRC).unwrap();
    assert_eq!(t.aligns, [Align::Left, Align::Right]);
    assert_eq!(t.rows, [vec!["Name", "Age"], vec!["Ann", "3"], vec!["Bob", ""]]);
    assert_eq!(t.write(), "| Name | Age |\n| :--- | ---: |\n| Ann | 3 |\n| Bob |  |\n");
}

#[test]
fn a_row_goes_in_with_empty_cells() {
    let mut t = TableText::parse(SRC).unwrap();
    t.insert_row(3);
    t.insert_row(1);
    assert_eq!(t.rows, [vec!["Name", "Age"], vec!["", ""], vec!["Ann", "3"], vec!["Bob", ""], vec!["", ""]]);
}

#[test]
fn a_cell_s_place_in_the_written_table() {
    let t = TableText::parse(SRC).unwrap();
    let text = t.write();
    let at = t.cell_offset(1, 1);
    assert_eq!(&text[at..at + 1], "3");
    let empty = t.cell_offset(2, 1);
    assert_eq!(&text[empty - 2..empty + 2], "|  |", "an empty cell's place is between its spaces");
}

#[test]
fn pipes_in_cells_stay_escaped_and_alignment_none_is_plain() {
    let t = TableText::parse("a | b\n--|--\nx \\| y | z\n").unwrap();
    assert_eq!(t.aligns, [Align::None, Align::None]);
    assert_eq!(t.write(), "| a | b |\n| --- | --- |\n| x \\| y | z |\n");
}

fn table(rows: &[&[&str]]) -> TableText {
    TableText { aligns: vec![Align::None; rows[0].len()], rows: rows.iter().map(|r| r.iter().map(|c| (*c).to_owned()).collect()).collect() }
}

#[test]
fn columns_go_in_and_out_with_their_alignment() {
    let mut t = TableText::parse(SRC).unwrap();
    t.insert_column(1);
    assert_eq!(t.aligns, [Align::Left, Align::None, Align::Right]);
    assert_eq!(t.rows[1], ["Ann", "", "3"]);
    t.delete_column(0);
    assert_eq!(t.aligns, [Align::None, Align::Right]);
    assert_eq!(t.rows[0], ["", "Age"]);
    t.delete_column(0);
    t.delete_column(0);
    assert_eq!(t.columns(), 1, "the last column stays");
}

#[test]
fn rows_go_out_the_header_passing_to_the_next_row_and_the_last_row_stays() {
    let mut t = table(&[&["h"], &["a"], &["b"]]);
    t.delete_row(1);
    assert_eq!(t.rows, [vec!["h"], vec!["b"]]);
    t.delete_row(0);
    assert_eq!(t.rows, [vec!["b"]]);
    t.delete_row(0);
    assert_eq!(t.rows, [vec!["b"]]);
}

#[test]
fn rows_and_columns_move() {
    let mut t = table(&[&["h1", "h2", "h3"], &["a1", "a2", "a3"], &["b1", "b2", "b3"]]);
    t.aligns[0] = Align::Right;
    t.move_row(2, 1);
    assert_eq!(t.rows[1], ["b1", "b2", "b3"]);
    t.move_column(0, 2);
    assert_eq!(t.rows[0], ["h2", "h3", "h1"]);
    assert_eq!(t.aligns, [Align::None, Align::None, Align::Right], "alignment moves with its column");
}

#[test]
fn sorting_by_a_column_keeps_the_header_and_compares_numbers_as_numbers() {
    let mut t = table(&[&["Name", "Qty"], &["pear", "10"], &["Apple", "9"], &["fig", "100"]]);
    t.sort_by(1, true);
    assert_eq!(t.rows.iter().map(|r| r[1].as_str()).collect::<Vec<_>>(), ["Qty", "9", "10", "100"]);
    t.sort_by(0, false);
    assert_eq!(t.rows.iter().map(|r| r[0].as_str()).collect::<Vec<_>>(), ["Name", "pear", "fig", "Apple"]);
}

#[test]
fn a_column_s_alignment_is_set() {
    let mut t = table(&[&["a", "b"]]);
    t.set_align(1, Align::Center);
    assert_eq!(t.write(), "| a | b |\n| --- | :---: |\n");
}
