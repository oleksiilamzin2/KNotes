use knotes_app::{Place, Refine};

fn tag(tag: &str) -> Place {
    Place::Tag(tag.into())
}
fn folder(folder: &str) -> Place {
    Place::Folder(folder.into())
}
fn space(space: &str) -> Place {
    Place::Space(space.into())
}

#[test]
fn a_query_is_taken_apart_into_its_words_and_what_narrows_them() {
    let r = Refine::parse(r#"radar "позовна давність" -чернетка tag:Робота folder:Авто budget"#);
    assert_eq!(r.words, "radar позовна давність budget", "a phrase's words are searched by too");
    assert_eq!(r.phrases, ["позовна давність"]);
    assert_eq!(r.without, ["чернетка"]);
    assert_eq!(r.places, [[tag("робота")], [folder("Авто")]], "a tag is compared in small letters");
    assert!(r.narrows());
    // The owner's own words for the keys, a `#` before a tag, a folder with spaces in quotes.
    let r = Refine::parse(r#"тег:#дім тека:"Мої проєкти/Радар" простір:"Моя робота" план"#);
    assert_eq!((r.places, r.words.as_str()), (vec![vec![tag("дім")], vec![folder("Мої проєкти/Радар")], vec![space("Моя робота")]], "план"));
    // Plain words narrow nothing; a `-` alone and a key with no value are words.
    let r = Refine::parse("план - tag: на-потім");
    assert_eq!((r.words.as_str(), r.narrows()), ("план - tag: на-потім", false));
    assert_eq!(Refine::parse("  ").words, "");
    assert!(Refine::parse(r#""a b" -c"#).places.is_empty(), "a phrase and a word left out say nothing of where");
    // A quote never closed keeps what follows as its phrase.
    assert_eq!(Refine::parse(r#"a "b c"#).phrases, ["b c"]);
}

#[test]
fn places_are_joined_by_and_and_or() {
    // `AND`, or nothing, between two: both. `OR`: one or the other, of any kinds.
    assert_eq!(Refine::parse("tag:a AND tag:b").places, [[tag("a")], [tag("b")]]);
    assert_eq!(Refine::parse("tag:a AND tag:b"), Refine::parse("tag:a tag:b"));
    assert_eq!(Refine::parse("space:Дім OR space:Робота").places, [[space("Дім"), space("Робота")]]);
    assert_eq!(Refine::parse("radar tag:a OR folder:B OR space:C AND tag:d план").places, [vec![tag("a"), folder("B"), space("C")], vec![tag("d")]]);
    assert_eq!(Refine::parse("radar tag:a OR folder:B OR space:C AND tag:d план").words, "radar план");
    // `AND` between words is left out; `OR` that joins no two places is a word, as are the
    // two in small letters and in quotes.
    assert_eq!(Refine::parse("radar AND budget").words, "radar budget");
    assert_eq!(Refine::parse("radar OR budget").words, "radar OR budget");
    let r = Refine::parse("tag:a OR budget and or");
    assert_eq!((r.places, r.words.as_str()), (vec![vec![tag("a")]], "OR budget and or"));
    assert_eq!(Refine::parse(r#"tag:a "OR" tag:b"#).places, [[tag("a")], [tag("b")]]);
    assert_eq!(Refine::parse("OR tag:a").words, "OR");
    assert_eq!(Refine::parse("tag:a AND OR tag:b").places, [[tag("a"), tag("b")]], "`AND` is as if not written");
    assert_eq!(Refine::parse("tag:a tag:b").tags(), ["a", "b"]);
}

#[test]
fn a_document_is_kept_by_its_places_its_phrases_and_the_words_it_must_not_hold() {
    let none = |_: &str| false;
    let r = Refine::parse(r#"folder:проєкти "next year" -draft"#);
    assert!(r.in_places("Проєкти/Radar.md", None, none) && r.in_places("Work/Проєкти/2026/Radar.md", None, none));
    assert!(!r.in_places("Radar.md", None, none) && !r.in_places("Проєкти нові/Radar.md", None, none), "a folder's whole name");
    assert!(Refine::parse("folder:Work/Проєкти").in_places("work/проєкти/a.md", None, none) && !Refine::parse("folder:Проєкти/Work").in_places("Work/Проєкти/a.md", None, none));
    // A space by its name, any case; a document is in one space, so two are asked with `OR`.
    let r = Refine::parse("space:дім");
    assert!(r.in_places("a.md", Some("Дім"), none) && !r.in_places("a.md", Some("Робота"), none) && !r.in_places("a.md", None, none));
    assert!(!Refine::parse("space:Дім space:Робота").in_places("a.md", Some("Дім"), none));
    assert!(Refine::parse("space:Дім OR space:Робота").in_places("a.md", Some("Робота"), none));
    // Tags as the caller knows them; kinds mixed.
    let has_a = |tag: &str| tag == "a";
    assert!(Refine::parse("tag:a").in_places("x.md", None, has_a) && !Refine::parse("tag:a tag:b").in_places("x.md", None, has_a));
    let r = Refine::parse("tag:b OR folder:Авто AND space:Дім");
    assert!(r.in_places("Авто/x.md", Some("Дім"), none) && !r.in_places("Авто/x.md", Some("Робота"), none) && !r.in_places("x.md", Some("Дім"), has_a));
    assert!(Refine::parse("план").in_places("a.md", None, none), "nothing asked, anywhere will do");
    // Phrases and the words left out.
    let r = Refine::parse(r#""next year" -draft"#);
    assert!(r.holds("Radar", "The budget grows Next Year.", true));
    assert!(!r.holds("Radar", "The budget grows Next Year.", false), "the case counts when asked");
    assert!(!r.holds("Radar", "It grows next\nyear.", true), "word for word");
    assert!(!r.holds("Radar draft", "It grows next year.", true) && !r.holds("Radar", "A DRAFT: next year.", true), "left out by its name or its text");
    assert!(Refine::parse("план").holds("a", "b", true), "nothing asked, nothing to hold");
}
