use knotes_app::{Refine, SearchMode, SearchRequest, search};
use knotes_core::{BlockHit, BoxError, FileKind, IndexChange, IndexWriter, MeaningSearch, NoteParser, Span, VaultFile};
use knotes_index::SqliteIndex;

/// The thresholds the tests' similarities are written for.
const CLOSE: knotes_tools::Closeness = knotes_tools::Closeness { near_enough: 0.25, behind_nearest: 0.10 };

/// Search by meaning as a list given beforehand (path, block span, similarity), in the scope
/// asked.
struct Near(Vec<(&'static str, u32, u32, f64)>);

impl MeaningSearch for Near {
    fn nearest(&self, _query: &str, limit: usize, scope: Option<&[String]>) -> Result<Vec<BlockHit>, BoxError> {
        Ok(self
            .0
            .iter()
            .filter(|(path, ..)| scope.is_none_or(|s| s.iter().any(|p| p == path)))
            .take(limit)
            .map(|(path, start, end, near)| BlockHit { path: path.to_string(), ord: 0, span: Span { start: *start, end: *end }, heading: None, score: -near })
            .collect())
    }

    fn near_document(&self, _path: &str, _limit: usize) -> Result<Vec<BlockHit>, BoxError> {
        Ok(Vec::new())
    }
}

fn index_of(dir: &std::path::Path, notes: &[(&str, &str)]) -> SqliteIndex {
    let mut index = SqliteIndex::open(dir).unwrap();
    let parsed: Vec<_> = notes.iter().map(|(_, t)| knotes_md::MarkdownParser::new().parse(t)).collect();
    let files: Vec<_> = notes.iter().map(|(p, _)| VaultFile { path: (*p).into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 }).collect();
    let changes: Vec<_> = files.iter().zip(notes).zip(&parsed).map(|((file, (_, text)), parsed)| IndexChange::Upsert { file, note: Some((text, parsed)) }).collect();
    index.apply(&changes).unwrap();
    index
}

#[test]
fn notes_holding_the_query_s_words_come_first_then_those_near_in_meaning_and_not_the_far_ones() {
    let dir = tempfile::tempdir().unwrap();
    let notes = [
        ("Аптечка.md", "Ліки: парацетамол, смекта."),
        ("Мама.md", "Тиск: тримає 130/85 на ліках. Ліки: лізиноприл."),
        ("Дорога.md", "Драмамін для Марка."),
        ("Піца.md", "Тісто на 2 піци."),
    ];
    let index = index_of(dir.path(), &notes);
    let whole = |i: usize| notes[i].1.len() as u32;
    let near = Near(vec![("Аптечка.md", 0, whole(0), 0.45), ("Дорога.md", 0, whole(2), 0.36), ("Мама.md", 0, whole(1), 0.30), ("Піца.md", 0, whole(3), 0.20)]);
    let request = SearchRequest { generation: 1, mode: SearchMode::Meaning, query: "ліки від тиску".into(), scope: None, titles: None, closeness: CLOSE, space: None, ignore_case: true, names_too: false, refine: Default::default(), spaces: None };
    let found = search(&request, Some(&index), Some(&near));
    let paths: Vec<&str> = found.content.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["Мама.md", "Аптечка.md", "Дорога.md"], "the note with the words first; the far one left out");
}

#[test]
fn notes_far_behind_the_nearest_are_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let notes = [("Ванна.md", "Ремонт: 87 450 грн."), ("Балкон.md", "Утеплення: 80 500 грн."), ("Хорватія.md", "Відпустка: 3 900 €.")];
    let index = index_of(dir.path(), &notes);
    let whole = |i: usize| notes[i].1.len() as u32;
    let near = Near(vec![("Ванна.md", 0, whole(0), 0.70), ("Балкон.md", 0, whole(1), 0.62), ("Хорватія.md", 0, whole(2), 0.55)]);
    let request = SearchRequest { generation: 1, mode: SearchMode::Meaning, query: "скільки коштувала переробка санвузла".into(), scope: None, titles: None, closeness: CLOSE, space: None, ignore_case: true, names_too: false, refine: Default::default(), spaces: None };
    let paths: Vec<String> = search(&request, Some(&index), Some(&near)).content.into_iter().map(|c| c.path).collect();
    assert_eq!(paths, ["Ванна.md", "Балкон.md"], "0.55 is too far behind 0.70");
}

#[test]
fn search_by_meaning_lists_the_nearest_notes_once_each_with_their_block() {
    let dir = tempfile::tempdir().unwrap();
    let notes = [("Cat.md", "Барсик їсть Royal Canin."), ("Shop.md", "Корм коту, сіль."), ("Work/Plan.md", "Квартальний план.")];
    let index = index_of(dir.path(), &notes);
    let whole = |i: usize| notes[i].1.len() as u32;
    let near = Near(vec![("Cat.md", 0, whole(0), 0.50), ("Cat.md", 0, 6, 0.48), ("Work/Plan.md", 0, whole(2), 0.47), ("Shop.md", 0, whole(1), 0.45)]);
    let request = |scope: Option<Vec<String>>| SearchRequest { generation: 1, mode: SearchMode::Meaning, query: "чим годувати кота".into(), scope, titles: None, closeness: CLOSE, space: None, ignore_case: true, names_too: false, refine: Default::default(), spaces: None };
    let found = search(&request(None), Some(&index), Some(&near));
    let paths: Vec<&str> = found.content.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["Cat.md", "Work/Plan.md", "Shop.md"], "nearest first, each note once");
    assert!(found.content[0].snippet.contains("Royal Canin"), "{:?}", found.content[0].snippet);
    let in_list = search(&request(Some(vec!["Cat.md".into(), "Shop.md".into()])), Some(&index), Some(&near));
    assert_eq!(in_list.content.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["Cat.md", "Shop.md"], "the selected list's notes only");
    assert!(search(&request(None), Some(&index), None).content.is_empty(), "no model: nothing found, nothing broken");
}

#[test]
fn in_a_space_search_by_meaning_finds_its_notes_apart_from_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let notes = [("Out1.md", "один"), ("Out2.md", "два"), ("Out3.md", "три"), ("Work/In.md", "робоче")];
    let index = index_of(dir.path(), &notes);
    let near = Near(vec![("Out1.md", 0, 4, 0.50), ("Out2.md", 0, 4, 0.48), ("Out3.md", 0, 4, 0.47), ("Work/In.md", 0, 4, 0.30)]);
    let request = |space: Option<Vec<String>>| SearchRequest { generation: 1, mode: SearchMode::Meaning, query: "щось".into(), scope: None, titles: None, closeness: CLOSE, space, ignore_case: true, names_too: false, refine: Default::default(), spaces: None };
    let paths = |space| search(&request(space), Some(&index), Some(&near)).content.into_iter().map(|c| c.path).collect::<Vec<_>>();
    assert!(!paths(None).contains(&"Work/In.md".to_string()), "in the whole vault the nearer notes raise the bar past it");
    assert_eq!(paths(Some(vec!["Work/In.md".into()])), ["Work/In.md", "Out1.md", "Out2.md", "Out3.md"], "the space's first, near enough among its own");
}

/// What `query` finds as the vault's search window asks it (names and text, any case), in the
/// alphabet's order.
fn found(index: &SqliteIndex, notes: &[(&str, &str)], spaces: &std::sync::Arc<knotes_spaces::Spaces>, query: &str, mode: SearchMode) -> String {
    let refine = Refine::parse(query);
    let titles = std::sync::Arc::new(knotes_search::TitleIndex::new(notes.iter().map(|(path, _)| (path.to_string(), Vec::new()))));
    let request = SearchRequest { generation: 1, mode, query: refine.words.clone(), scope: None, titles: Some(titles), closeness: CLOSE, space: None, ignore_case: true, names_too: true, refine, spaces: Some(spaces.clone()) };
    let response = search(&request, Some(index), None);
    let mut paths: Vec<String> = response.titles.into_iter().chain(response.content.into_iter().map(|c| c.path)).collect();
    paths.sort();
    paths.dedup();
    paths.join(", ")
}

#[test]
fn a_search_is_narrowed_by_a_phrase_a_word_left_out_a_tag_a_folder_and_a_space() {
    let dir = tempfile::tempdir().unwrap();
    let notes = [
        ("Projects/Radar.md", "# Radar\n\nThe radar budget grows next year. #work\n"),
        ("Projects/Draft.md", "Radar draft: the year next to budget. #work/old\n"),
        ("Home/Сад.md", "Radar is mentioned here once. #garden\n"),
    ];
    let index = index_of(dir.path(), &notes);
    let rules = [
        (String::new(), r#"{"spaces": [{"id": "work", "name": "Робота", "color": "blue"}, {"id": "home", "name": "Дім", "color": "green"}]}"#.to_owned()),
        ("Projects".to_owned(), r#"{"space": "work"}"#.to_owned()),
        ("Home".to_owned(), r#"{"space": "home"}"#.to_owned()),
    ];
    let spaces = std::sync::Arc::new(knotes_spaces::Spaces::from_texts(&rules));
    let found = |query: &str| found(&index, &notes, &spaces, query, SearchMode::Content);
    assert_eq!(found("radar"), "Home/Сад.md, Projects/Draft.md, Projects/Radar.md");
    assert_eq!(found(r#"radar "next year""#), "Projects/Radar.md", "word for word: Draft has «year next»");
    assert_eq!(found("radar -draft"), "Home/Сад.md, Projects/Radar.md", "left out by its name and its text");
    assert_eq!(found("radar tag:work"), "Projects/Draft.md, Projects/Radar.md", "a tag with those under it");
    assert_eq!(found("radar тег:#work/old"), "Projects/Draft.md");
    assert_eq!(found("radar folder:home"), "Home/Сад.md");
    assert_eq!(found("radar space:дім"), "Home/Сад.md");
    assert_eq!(found("radar простір:Робота -draft"), "Projects/Radar.md");
    assert_eq!(found("radar space:Nowhere"), "", "a space the vault has not");
    // `OR` between two places: one or the other; `AND`, or nothing: both.
    assert_eq!(found("radar space:Дім OR space:Робота"), "Home/Сад.md, Projects/Draft.md, Projects/Radar.md");
    assert_eq!(found("radar space:Дім AND space:Робота"), "", "no document is in two spaces");
    assert_eq!(found("radar tag:garden OR tag:work/old"), "Home/Сад.md, Projects/Draft.md");
    assert_eq!(found("radar tag:work AND tag:work/old"), "Projects/Draft.md");
    assert_eq!(found("radar tag:work tag:work/old"), "Projects/Draft.md");
    assert_eq!(found("radar tag:garden OR folder:projects AND -draft"), "Home/Сад.md, Projects/Radar.md", "of different kinds");
    assert_eq!(found("folder:home OR folder:projects -draft"), "Home/Сад.md, Projects/Radar.md");
    assert_eq!(found("radar AND budget"), "Projects/Draft.md, Projects/Radar.md", "`AND` between words is as if not written");
    // `OR` between two words: the documents with either, each once.
    assert_eq!(found("grows OR mentioned"), "Home/Сад.md, Projects/Radar.md");
    assert_eq!(found("grows OR mentioned radar space:Дім"), "Home/Сад.md");
    assert_eq!(found("radar OR budget"), "Home/Сад.md, Projects/Draft.md, Projects/Radar.md");
    // Where to look and no words: every document there. A word left out alone says nothing
    // of where: nothing listed.
    assert_eq!(found("space:Робота"), "Projects/Draft.md, Projects/Radar.md");
    assert_eq!(found("tag:garden"), "Home/Сад.md");
    assert_eq!(found("folder:Projects tag:garden"), "");
    assert_eq!(found("-draft"), "");
    // The names alone are narrowed as the text is.
    let names = |query: &str| self::found(&index, &notes, &spaces, query, SearchMode::Files);
    assert_eq!(names("radar"), "Projects/Radar.md");
    assert_eq!(names("radar -budget"), "", "left out by its text though found by its name");
    assert_eq!(names("сад OR draft space:Дім"), "Home/Сад.md");
}
