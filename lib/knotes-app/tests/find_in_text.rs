use knotes_app::find_all;

#[test]
fn every_occurrence_is_found_whatever_its_case() {
    let text = "Пункт і пункти, ПУНКТ; punkt";
    let found: Vec<&str> = find_all(text, "пункт").into_iter().map(|r| &text[r]).collect();
    assert_eq!(found, ["Пункт", "пункт", "ПУНКТ"]);
}

#[test]
fn occurrences_do_not_overlap_and_an_empty_query_finds_nothing() {
    assert_eq!(find_all("aaaa", "aa"), [0..2, 2..4]);
    assert!(find_all("text", "").is_empty());
    assert!(find_all("text", "zz").is_empty());
}
