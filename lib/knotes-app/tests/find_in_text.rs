use knotes_app::{find_all, find_all_cased};

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

#[test]
fn with_case_kept_only_the_query_s_own_case_is_found() {
    let text = "Пункт перший, пункт другий, ПУНКТ третій";
    assert_eq!(find_all_cased(text, "пункт", true).len(), 3);
    let found: Vec<&str> = find_all_cased(text, "пункт", false).into_iter().map(|r| &text[r]).collect();
    assert_eq!(found, ["пункт"]);
    assert!(find_all_cased(text, "", false).is_empty());
}
