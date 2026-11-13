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
