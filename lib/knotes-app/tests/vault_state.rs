use std::sync::Arc;

use knotes_app::{Clock, ContentMatch, ListItem, SidebarItem, SidebarMode, VaultState, folder_tree, tag_tree};
use knotes_core::{FileKind, IndexChange, IndexWriter, NoteParser, NoteRow, TrashEntry, VaultFile};
use knotes_index::SqliteIndex;

const CLOCK: Clock = Clock { now_ns: 1_794_470_400_000_000_000, utc_offset_s: 7200 };
const INBOX: &str = "Вхідні";

type State = VaultState<SqliteIndex, ()>;

const NOTES: [(&str, &str); 5] = [
    ("Home/Сад.md", "Сад і город. #garden\n\n#@plants rose::троянда ^c-1\n"),
    ("Work/Plan.md", "Квартальний план. #work\n"),
    ("Work/2026/Radar.md", "Radar. #work/radar\n\n#@plants oak::дуб ^c-2\n"),
    ("Top.md", "No tag here.\n"),
    ("Top2.md", "Nor here.\n"),
];

fn index_of(dir: &std::path::Path, notes: &[(&str, &str)]) -> SqliteIndex {
    let mut index = SqliteIndex::open(dir).unwrap();
    let parsed: Vec<_> = notes.iter().map(|(_, t)| knotes_md::MarkdownParser::new().parse(t)).collect();
    let files: Vec<_> = notes.iter().map(|(p, _)| VaultFile { path: (*p).into(), kind: FileKind::Markdown, size: 1, mtime_ns: 1 }).collect();
    let changes: Vec<_> = files.iter().zip(notes).zip(&parsed).map(|((file, (_, text)), parsed)| IndexChange::Upsert { file, note: Some((text, parsed)) }).collect();
    index.apply(&changes).unwrap();
    index
}

/// Two spaces: «Дім» with the folder Home and «Робота» with Work.
fn spaces() -> Arc<knotes_spaces::Spaces> {
    let rules = [
        (String::new(), r#"{"spaces": [{"id": "home", "name": "Дім", "color": "green"}, {"id": "work", "name": "Робота", "color": "blue"}]}"#.to_owned()),
        ("Home".to_owned(), r#"{"space": "home"}"#.to_owned()),
        ("Work".to_owned(), r#"{"space": "work"}"#.to_owned()),
    ];
    Arc::new(knotes_spaces::Spaces::from_texts(&rules))
}

/// The state after a snapshot of `NOTES`, the index read from `dir`; the vault has two spaces,
/// «Дім» with the folder Home and «Робота» with Work.
fn state(dir: &std::path::Path) -> State {
    let index = index_of(dir, &NOTES);
    let mut rows: Vec<NoteRow> = NOTES.iter().map(|(path, _)| NoteRow { path: (*path).into(), mtime_ns: 1, snippet: String::new() }).collect();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    State {
        spaces: spaces(),
        rows: Arc::new(rows),
        folders: Arc::new(folder_tree(NOTES.iter().map(|(path, _)| *path))),
        tags: Arc::new(tag_tree(&[("garden".into(), 1), ("work".into(), 2), ("work/radar".into(), 1)])),
        decks: Arc::new(tag_tree(&[("plants".into(), 2)])),
        untagged: Arc::new(["Top.md".to_owned(), "Top2.md".to_owned()].into()),
        tagged: Arc::new(vec![("garden".into(), "Home/Сад.md".into()), ("work".into(), "Work/Plan.md".into()), ("work/radar".into(), "Work/2026/Radar.md".into())]),
        reader: Some(index),
        ..State::default()
    }
}

/// The notes of a list, in its order; `|` where the rest of the vault begins.
fn notes(state: &State, items: &[ListItem]) -> String {
    let named: Vec<&str> = items
        .iter()
        .filter_map(|item| match item {
            ListItem::Note(n) | ListItem::Found(n, _) => Some(state.rows[*n as usize].path.as_str()),
            ListItem::Rest => Some("|"),
            _ => None,
        })
        .collect();
    named.join(" ")
}

fn listed(state: &State) -> String {
    let among = state.notes_listed();
    notes(state, &state.listed(among.as_deref(), None, &CLOCK).items)
}

#[test]
fn the_list_holds_the_notes_of_the_row_selected() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    assert_eq!(listed(&state).split(' ').count(), 5, "all notes: {}", listed(&state));
    state.selection = SidebarItem::Folder("Work".into());
    assert_eq!(listed(&state), "Work/Plan.md", "a folder's own notes, as its rules say (none here: those under it are not listed)");
    // A tag with those under it, a deck's notes and those with no tag: asked of the index.
    state.selection = SidebarItem::Tag("work".into());
    assert_eq!(listed(&state), "Work/Plan.md Work/2026/Radar.md", "by their names");
    state.selection = SidebarItem::Deck("plants".into());
    assert_eq!(listed(&state), "Work/2026/Radar.md Home/Сад.md");
    state.selection = SidebarItem::Untagged;
    assert_eq!(listed(&state), "Top.md Top2.md");
    // Their list options last the session, under the row's key; a folder's are its rules'.
    let among = state.notes_listed();
    assert_eq!(state.listed(among.as_deref(), None, &CLOCK).session_key, Some(SidebarItem::Untagged.key()));
    state.selection = SidebarItem::Folder("Work".into());
    assert_eq!(state.listed(None, None, &CLOCK).session_key, None);
}

#[test]
fn in_a_space_the_list_holds_its_notes_then_the_rest_and_a_deck_only_its_part() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.space = Some("work".into());
    state.fit_space();
    assert_eq!(listed(&state), "Work/Plan.md Work/2026/Radar.md | Top.md Top2.md Home/Сад.md", "the space's, then the rest of the vault's");
    state.selection = SidebarItem::Rest;
    assert_eq!(listed(&state), "Top.md Top2.md Home/Сад.md", "«Решта бази»: those outside");
    state.selection = SidebarItem::Folder("Home".into());
    assert_eq!(listed(&state), "Home/Сад.md", "a folder outside the space lists as the rest does");
    state.selection = SidebarItem::Deck("plants".into());
    assert_eq!(listed(&state), "Work/2026/Radar.md", "the deck is the space's part of it");
    // The sidebar: the space's tags, the rest's under «Решта бази» (kept for the list: a tag
    // there lists outside the space).
    let (sidebar, rest_tags) = state.sidebar(INBOX);
    assert_eq!(rest_tags, Some(["garden".to_owned()].into()));
    assert_eq!(sidebar.count(&SidebarItem::AllNotes), Some(2), "the space's notes");
    state.rest_tags = rest_tags.unwrap();
    state.sidebar_mode = SidebarMode::Tags;
    assert!(state.sidebar(INBOX).0.children(None).contains(&SidebarItem::Tag("work".into())));
    // The decks have no such row: what the sidebar held of the rest's tags stays.
    state.sidebar_mode = SidebarMode::Cards;
    let (decks, rest_tags) = state.sidebar(INBOX);
    assert_eq!((decks.count(&SidebarItem::Deck("plants".into())), rest_tags), (Some(2), None));
    // A space the vault has no more: the whole vault again.
    state.space = Some("gone".into());
    state.fit_space();
    assert!(state.space.is_none() && state.space_folders.is_none());
    state.selection = SidebarItem::AllNotes;
    assert!(!listed(&state).contains('|'));
}

#[test]
fn a_search_typed_lists_its_finds_and_covers_the_list_s_notes() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let titles = ["Work/Plan.md".to_owned(), "Top.md".to_owned()];
    let content = [ContentMatch { path: "Home/Сад.md".into(), snippet: "Сад і город".into() }];
    let found = Some((titles.as_slice(), content.as_slice()));
    // Nothing typed: the answer of an earlier search is not listed.
    assert_eq!(notes(&state, &state.listed(None, found, &CLOCK).items).split(' ').count(), 5);
    assert!(state.listed(None, found, &CLOCK).scope.is_none());
    // Typed over all notes: the finds, the whole vault searched.
    state.search = "план".into();
    let all = state.listed(None, found, &CLOCK);
    assert_eq!((notes(&state, &all.items).as_str(), all.scope, all.space_scope), ("Work/Plan.md Top.md Home/Сад.md", None, None));
    // Over a folder: its finds, and its notes are what is searched.
    state.selection = SidebarItem::Folder("Work".into());
    let folder = state.listed(None, found, &CLOCK);
    assert_eq!(notes(&state, &folder.items), "Work/Plan.md");
    assert_eq!(folder.scope, Some(vec!["Work/Plan.md".to_owned()]), "the folder's own notes, as it lists them");
    // In a space, over all notes: its part is searched apart (ADR 0030).
    state.selection = SidebarItem::AllNotes;
    state.space = Some("home".into());
    state.fit_space();
    let space = state.listed(None, found, &CLOCK);
    assert_eq!(notes(&state, &space.items), "Home/Сад.md | Work/Plan.md Top.md", "the space's finds first");
    assert_eq!((space.scope, space.space_scope), (None, Some(vec!["Home/Сад.md".to_owned()])));
}

#[test]
fn the_sidebar_has_a_row_only_for_what_is_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let exists = |state: &State, item: SidebarItem| state.exists(&item, INBOX);
    assert!(exists(&state, SidebarItem::AllNotes) && exists(&state, SidebarItem::Untagged));
    assert!(exists(&state, SidebarItem::Folder("Work/2026".into())) && !exists(&state, SidebarItem::Folder("Gone".into())));
    assert!(exists(&state, SidebarItem::Tag("work/radar".into())) && !exists(&state, SidebarItem::Tag("gone".into())));
    assert!(exists(&state, SidebarItem::Deck("plants".into())) && !exists(&state, SidebarItem::Deck("gone".into())));
    // The inbox's row shows before its folder exists; in a space only if it is the space's.
    assert!(exists(&state, SidebarItem::Folder(INBOX.into())));
    assert!(!exists(&state, SidebarItem::Rest), "no rest without a space");
    state.space = Some("work".into());
    state.fit_space();
    assert!(exists(&state, SidebarItem::Rest) && !exists(&state, SidebarItem::Folder(INBOX.into())));
    assert!(exists(&state, SidebarItem::Folder("Home".into())), "every folder shows, in the space or under the rest");
    // The recently deleted: a row while there are any.
    assert!(!exists(&state, SidebarItem::Deleted));
    state.trash = Arc::new(vec![TrashEntry { id: "1".into(), path: "Old.md".into(), folder: false, deleted_ms: 1, notes: 1 }]);
    assert!(exists(&state, SidebarItem::Deleted));
    // The row selected stays while it is there; gone, all notes are selected.
    state.selection = SidebarItem::Deleted;
    assert_eq!(state.fit_selection(INBOX), SidebarItem::Deleted);
    state.trash = Arc::default();
    assert_eq!((state.fit_selection(INBOX), state.selection.clone()), (SidebarItem::AllNotes, SidebarItem::AllNotes));
}

#[test]
fn a_deck_asks_its_due_cards_and_the_day_s_new_ones_and_in_a_space_those_of_its_notes() {
    use knotes_cards::{DeckSettings, Fsrs, Grade, review_line};
    use knotes_core::CardIndex;
    const NOW: i64 = 1_800_000_000;
    const DAY_START: i64 = NOW - 3_600;
    let notes = [("Home/a.md", "#@en cat::кіт ^c-1\n#@en dog::пес ^c-2\n#@en/verbs go::йти ^c-3\n"), ("Work/b.md", "#@en cow::корова ^c-4\n#@de eins::один ^c-5\n")];
    let dir = tempfile::tempdir().unwrap();
    let mut index = index_of(dir.path(), &notes);
    let all = DeckSettings::default();
    let two = DeckSettings::parse(r#"{"en": {"new_a_day": 2}}"#);
    // cat was graded Easy half an hour ago: no more new today, and not due for days.
    index.take_reviews("mac", 0, &review_line(NOW - 1_800, "c-1", Grade::Easy), &Fsrs::default(), &all).unwrap();
    let counts = index.deck_counts(NOW).unwrap();
    let mut state = State { spaces: spaces(), reader: Some(index), ..State::default() };
    let ids = |cards: Vec<knotes_core::DueCard>| cards.into_iter().map(|c| c.id.unwrap_or_default()).collect::<Vec<_>>().join(" ");
    // The whole vault: the deck with those under it, the new ones in their notes' order.
    assert_eq!(ids(state.due_cards("en", &all, NOW, DAY_START, 200)), "c-2 c-3 c-4");
    assert_eq!(ids(state.due_cards("en", &all, NOW, DAY_START, 2)), "c-2 c-3", "no more than asked for at once");
    assert_eq!(ids(state.due_cards("en", &two, NOW, DAY_START, 200)), "c-2", "two new a day, and cat was one of today's");
    assert_eq!(ids(state.due_cards("en", &two, NOW, NOW, 200)), "c-2 c-3", "a new day: both again");
    assert_eq!(ids(state.cards_ahead("en", NOW, 200)), "c-1", "ahead of the schedule: the one reviewed and not due");
    let tree = |state: &State, settings: &DeckSettings| state.deck_tree(&counts, settings, NOW, DAY_START).iter().map(|deck| format!("{} {}", deck.tag, deck.notes)).collect::<Vec<_>>().join(", ");
    assert_eq!(tree(&state, &all), "de 1, en 3");
    assert_eq!(tree(&state, &two), "de 1, en 1", "what each still asks today");
    assert_eq!(state.deck_stats("en", DAY_START).map(|s| (s.cards, s.fresh)), Some((4, 3)));
    assert!(state.deck_stats("none", DAY_START).is_none());
    // In «Дім»: the cards of its notes; cow and the German deck are the other space's.
    state.space = Some("home".into());
    assert_eq!(ids(state.due_cards("en", &all, NOW, DAY_START, 200)), "c-2 c-3");
    assert_eq!(ids(state.due_cards("en", &two, NOW, DAY_START, 200)), "c-2");
    assert_eq!(ids(state.cards_ahead("en", NOW, 200)), "c-1");
    assert_eq!(tree(&state, &all), "en 2", "no German deck here");
    assert_eq!(state.deck_stats("en", DAY_START).map(|s| s.cards), Some(3));
    state.space = Some("work".into());
    assert_eq!(ids(state.due_cards("en", &two, NOW, DAY_START, 200)), "c-4", "none of its new cards was asked today");
    assert_eq!(ids(state.cards_ahead("en", NOW, 200)), "");
    assert_eq!(tree(&state, &all), "de 1, en 1");
    assert!(state.deck_stats("en/verbs", DAY_START).is_none(), "no card of that deck in this space");
    // Before the index is open: nothing, and nothing broken.
    let empty = State::default();
    assert!(empty.due_cards("en", &all, NOW, DAY_START, 200).is_empty() && empty.cards_ahead("en", NOW, 200).is_empty() && empty.deck_stats("en", DAY_START).is_none());
}
