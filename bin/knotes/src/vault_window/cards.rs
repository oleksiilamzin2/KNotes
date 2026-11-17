//! The notes' cards in the window (ADR 0037): a card made from what is selected in the note,
//! and a deck's review in the note column.

use std::collections::VecDeque;

use knotes_cards::{DeckSettings, Fsrs, Grade, Memory, asks, card_id, give_id, make_card, review_line, scan_cards};
use knotes_app::deck_rows;
use knotes_core::{CardIndex, CardRow, CardState, DueCard};

use super::*;
use crate::review_view::{Asked, Shown};

/// What a tab's path starts with when the tab is a deck's review, not a document (no vault
/// path holds `//`): `deck://мови/англійська`.
pub(crate) const DECK_TAB: &str = "deck://";

/// The deck a tab reviews, if its path is a deck's.
pub(crate) fn deck_of_tab(path: &str) -> Option<&str> {
    path.strip_prefix(DECK_TAB)
}

/// Cards a review takes at once from the index; more wait for the next.
const REVIEW_CARDS: usize = 200;
const DAY: f64 = 86_400.0;

/// What a snapshot's decks were counted from: while all of it is the same, the count is too.
/// The vault's own counts change with a card written, removed or reviewed; its files with a
/// note moved between spaces; the spaces' tags are counted anew when a space's folders change.
pub(super) struct DecksCounted {
    decks: Arc<Vec<knotes_core::DeckCount>>,
    catalog: Arc<Vec<knotes_core::CatalogEntry>>,
    space_tags: Arc<Vec<SpaceTags>>,
    space: Option<String>,
    /// The decks' settings as their file has them, and the day (its new cards count from it).
    settings: String,
    day: i64,
}

impl DecksCounted {
    pub(super) fn of(snapshot: &Snapshot, state: &State) -> Self {
        Self {
            decks: Arc::clone(&snapshot.decks),
            catalog: Arc::clone(&snapshot.catalog),
            space_tags: Arc::clone(&snapshot.space_tags),
            space: state.space.clone(),
            settings: state.vault.as_ref().and_then(|vault| FsVault::new(vault).read_deck_settings().ok()).unwrap_or_default(),
            day: day_start(),
        }
    }
}

impl PartialEq for DecksCounted {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.decks, &other.decks)
            && Arc::ptr_eq(&self.catalog, &other.catalog)
            && Arc::ptr_eq(&self.space_tags, &other.space_tags)
            && self.space == other.space
            && self.settings == other.settings
            && self.day == other.day
    }
}

/// A deck's review under way.
pub(super) struct Review {
    deck: String,
    queue: VecDeque<DueCard>,
    current: Option<Current>,
    reviewed: usize,
}

/// The card shown.
struct Current {
    card: DueCard,
    front: String,
    back: String,
    /// Its answer shows.
    answered: bool,
}

/// When today began, by the Mac's clock (seconds since 1970): a day's new cards count from it.
fn day_start() -> i64 {
    let offset = objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT() as i64;
    (now() + offset).div_euclid(86_400) * 86_400 - offset
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Where the card would stand graded `grade` at `at`: from what the index knows of it (none:
/// a new card), by its deck's chance of recall and steps.
fn graded(settings: &DeckSettings, deck: &str, state: Option<CardState>, at: i64, grade: Grade) -> knotes_cards::Schedule {
    let fsrs = Fsrs { retention: settings.retention(deck), ..Fsrs::default() };
    let before = state.map(|s| knotes_cards::Schedule::standing(Memory { stability: s.stability, difficulty: s.difficulty }, s.last, s.step, s.relearning));
    knotes_cards::review(&fsrs, &settings.steps(deck), before.as_ref(), at, grade)
}

/// When a card would come back, said short: «10 хв» while it is learned, then «3 д», «2 міс»,
/// «1,5 р».
fn comes_back(after: &knotes_cards::Schedule) -> String {
    match after.step {
        Some(_) => strings::review_wait(after.due - after.last),
        None => strings::review_interval(((after.due - after.last) as f64 / DAY).round()),
    }
}

impl VaultWindow {
    /// Edit → Make a Card (and the note's context menu): what is selected in the note becomes a
    /// card. Its deck is asked, and its question unless what is selected asks already (a
    /// `question::answer`, a cloze); the card is written in place, as typed (Undo takes it
    /// back), and saved.
    pub fn make_card(&self) {
        let Some(path) = self.open_path() else { return };
        if self.editing.borrow().as_ref().is_some_and(|e| e.read_only) {
            return;
        }
        let text = self.note_text.string().to_string();
        let range = self.note_text.selectedRange();
        let (start, end) = (knotes_editor::byte_offset(&text, range.location), knotes_editor::byte_offset(&text, range.location + range.length));
        let selected = text[start..end].trim();
        if selected.is_empty() {
            self.tell_failed(strings::CARD_NOTHING_SELECTED, &strings::CARD_NOTHING_SELECTED_DETAILS);
            return;
        }
        let need_question = !asks(selected);
        let decks = deck_paths(&self.state.borrow().decks);
        let last = self.last_deck.borrow().clone();
        let Some((deck, question)) = rename_dialog::ask_card(MainThreadMarker::from(&*self.window), &decks, &last, need_question) else { return };
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let Some((replace, with)) = make_card(&text, start..end, &deck, question.as_deref(), &card_id(seed)) else {
            self.tell_failed(strings::CARD_NOT_MADE, &strings::CARD_NOT_MADE_DETAILS);
            return;
        };
        *self.last_deck.borrow_mut() = deck;
        let new = format!("{}{with}{}", &text[..replace.start], &text[replace.end..]);
        super::ai::replace_changed(&self.note_text, &text, &new);
        if let Err((_, e)) = self.write_all() {
            self.tell_save_failed(&path, &e);
        }
    }

    /// A deck chosen in the sidebar: the note column shows how many of its cards wait, and a
    /// button to start.
    pub(super) fn show_deck(&self, deck: &str) {
        // A tab of its own beside the one shown (owner's request): the document open keeps its
        // tab, and the review is come back to as one.
        self.remember_place();
        if !self.show_nothing() {
            self.stay();
            return;
        }
        self.tabs.borrow_mut().open_beside(&format!("{DECK_TAB}{deck}"));
        self.save_tabs();
        if self.state.borrow().sidebar_mode == SidebarMode::Tabs {
            self.rebuild_sidebar();
            self.highlight(&SidebarItem::Gap);
        }
        *self.review_deck.borrow_mut() = Some(deck.to_owned());
        let cards = self.due_cards(deck);
        self.show_list_column(true);
        *self.review.borrow_mut() = None;
        self.show_waiting(deck, &cards);
        self.update_navigation();
    }

    /// The review goes: another list is chosen, or a document opened.
    pub(super) fn hide_review(&self) {
        self.show_list_column(true);
        *self.review.borrow_mut() = None;
        *self.review_deck.borrow_mut() = None;
        self.review_view.show(None);
    }

    /// In a space (ADR 0030), the cards written in its notes: its decks are those, counted and
    /// reviewed from them (owner's request: decks follow the spaces by their files). None for
    /// the whole vault, which the index counts itself.
    fn space_rows(&self, state: &mut State) -> Option<Vec<CardRow>> {
        let id = state.space.clone()?;
        self.ensure_reader(state);
        let mut rows = state.reader.as_ref().and_then(|r| r.card_rows(None, None).ok()).unwrap_or_default();
        rows.retain(|row| state.spaces.holds(&row.path, &id));
        Some(rows)
    }

    /// The cards `deck` asks now: those due, and of the new as many as it still asks today.
    fn due_cards(&self, deck: &str) -> Vec<DueCard> {
        let mut state = self.state.borrow_mut();
        let rows = self.space_rows(&mut state);
        self.ensure_reader(&mut state);
        let Some(reader) = state.reader.as_ref() else { return Vec::new() };
        // Asked of the index, not as the last snapshot had it: a review just done counts.
        let under = format!("{deck}/");
        let introduced = match &rows {
            Some(rows) => deck_rows::introduced(rows, day_start()),
            None => reader.introduced(day_start()).unwrap_or_default(),
        };
        let today: usize = introduced.iter().filter(|(d, _)| d == deck || d.starts_with(&under)).map(|(_, n)| n).sum();
        let new = (self.deck_settings.borrow().new_a_day(deck) as usize).saturating_sub(today);
        match &rows {
            Some(rows) => deck_rows::due_cards(rows, deck, now(), REVIEW_CARDS, new),
            None => reader.due_cards(Some(deck), now(), REVIEW_CARDS, new).unwrap_or_default(),
        }
    }

    /// The decks' tree with what each asks now, from the last snapshot's counts (in a space,
    /// its notes' cards), the vault's settings as its file has them, and the new cards today
    /// has taken.
    pub(super) fn count_decks(&self, state: &mut State) -> Vec<TagNode> {
        let settings = state.vault.clone().and_then(|vault| FsVault::new(&vault).read_deck_settings().ok()).map(|text| DeckSettings::parse(&text)).unwrap_or_default();
        let new_a_day = |deck: &str| settings.new_a_day(deck) as usize;
        let tree = match self.space_rows(state) {
            Some(rows) => knotes_app::deck_tree(&deck_rows::deck_counts(&rows, now()), &deck_rows::introduced(&rows, day_start()), &new_a_day),
            None => {
                self.ensure_reader(state);
                let introduced = state.reader.as_ref().and_then(|r| r.introduced(day_start()).ok()).unwrap_or_default();
                knotes_app::deck_tree(&self.deck_counts.borrow(), &introduced, &new_a_day)
            }
        };
        *self.deck_settings.borrow_mut() = settings;
        tree
    }

    /// The decks counted again and shown: the window's space changed, or a deck's settings.
    pub(super) fn recount_decks(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.decks = Arc::new(self.count_decks(&mut state));
        }
        if self.state.borrow().sidebar_mode == SidebarMode::Cards {
            self.rebuild_sidebar();
        }
        // A deck waiting to be started says how many cards it asks now.
        let waiting = self.review_deck.borrow().clone().filter(|_| self.review.borrow().is_none());
        if let Some(deck) = waiting {
            let cards = self.due_cards(&deck);
            self.show_waiting(&deck, &cards);
        }
    }

    /// A deck's menu → its settings: asked, written to the vault's file (the session schedules
    /// the cards again by them), and the decks counted again at once.
    pub(super) fn set_deck(&self, deck: &str) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        // As the file has them now: another Mac may have written since.
        let mut settings = files.read_deck_settings().map(|text| DeckSettings::parse(&text)).unwrap_or_default();
        let Some(asked) = rename_dialog::ask_deck_settings(MainThreadMarker::from(&*self.window), deck, settings.own(deck), settings.inherited(deck)) else { return };
        settings.set(deck, asked);
        if let Err(e) = files.write_deck_settings(&settings.to_text()) {
            self.tell_failed(strings::DECK_SETTINGS_NOT_SAVED, &e.to_string());
            return;
        }
        self.recount_decks();
    }

    /// `deck`'s reviewed cards not due yet, the soonest due first; in a space, of its notes'.
    fn cards_ahead(&self, deck: &str, limit: usize) -> Vec<DueCard> {
        let mut state = self.state.borrow_mut();
        if let Some(rows) = self.space_rows(&mut state) {
            return deck_rows::cards_ahead(&rows, deck, now(), limit);
        }
        self.ensure_reader(&mut state);
        state.reader.as_ref().and_then(|r| r.cards_ahead(deck, now(), limit).ok()).unwrap_or_default()
    }

    /// A deck not started: how many of its cards wait, and its statistics.
    fn show_waiting(&self, deck: &str, cards: &[DueCard]) {
        let stats = {
            let mut state = self.state.borrow_mut();
            self.ensure_reader(&mut state);
            // In a space, of its notes' cards.
            let (spaces, space) = (Arc::clone(&state.spaces), state.space.clone());
            let holds = |path: &str| space.as_ref().is_none_or(|id| spaces.holds(path, id));
            state.reader.as_ref().and_then(|r| r.deck_stats(deck, day_start(), &holds).ok()).filter(|stats| stats.cards > 0)
        };
        let said = stats.as_ref().map(strings::deck_stats).unwrap_or_default();
        let ahead = self.cards_ahead(deck, REVIEW_CARDS);
        self.review_view.show(Some(Shown::Waiting { deck: deck.to_owned(), cards: cards.len(), more: cards.len() == REVIEW_CARDS, stats: said, charts: stats, ahead: ahead.len(), ahead_more: ahead.len() == REVIEW_CARDS }));
    }

    /// The waiting deck's statistics (probe mode).
    pub fn describe_deck_stats(&self) -> String {
        self.review_view.describe_stats()
    }

    /// A button of the review.
    pub(super) fn review_asked(&self, asked: Asked) {
        match asked {
            Asked::Start | Asked::StartAhead => {
                let Some(deck) = self.review_deck.borrow().clone() else { return };
                let mut queue: VecDeque<DueCard> = self.due_cards(&deck).into();
                // Ahead of the schedule: after what is due, the cards not due yet, the soonest first.
                if asked == Asked::StartAhead {
                    queue.extend(self.cards_ahead(&deck, REVIEW_CARDS.saturating_sub(queue.len())));
                }
                *self.review.borrow_mut() = Some(Review { deck, queue, current: None, reviewed: 0 });
                self.next_card();
            }
            Asked::ShowAnswer => {
                if let Some(current) = self.review.borrow_mut().as_mut().and_then(|r| r.current.as_mut()) {
                    current.answered = true;
                }
                self.show_review();
            }
            Asked::Grade(grade) => self.grade_card(grade),
            // A link in the card's text: the note it names (in a tab beside the review's), or
            // the web's address in the browser.
            Asked::Link(knotes_cards::CardLink::Note(target)) => self.open_chat_link(&target),
            Asked::Link(knotes_cards::CardLink::Web(address)) => super::links::open_url(&address),
            Asked::OpenNote => {
                let at = self.review.borrow().as_ref().and_then(|r| r.current.as_ref()).map(|c| (c.card.path.clone(), c.card.start as usize));
                if let Some((path, start)) = at
                    && self.open_note(&path, true)
                {
                    let text = self.note_text.string().to_string();
                    let offset = knotes_editor::utf16_offsets(&text, &[start.min(text.len())])[0];
                    self.note_text.setSelectedRange(NSRange::new(offset, 0));
                    self.scroll_note_to(offset);
                }
            }
        }
    }

    /// The next card of the review that is still written where the index found it; none left,
    /// the review is done.
    fn next_card(&self) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        loop {
            let Some(card) = self.review.borrow_mut().as_mut().and_then(|r| r.queue.pop_front()) else {
                if let Some(review) = self.review.borrow_mut().as_mut() {
                    review.current = None;
                }
                break;
            };
            // As the note holds it now: by its id, or where it was.
            let written = files.read_utf8(&card.path).ok().and_then(|text| {
                scan_cards(&text).into_iter().find(|c| match &card.id {
                    Some(id) => c.id.as_ref() == Some(id),
                    None => c.id.is_none() && c.span.start == card.start as usize,
                })
            });
            if let (Some(written), Some(review)) = (written, self.review.borrow_mut().as_mut()) {
                let card = DueCard { start: written.span.start as u32, end: written.span.end as u32, ..card };
                review.current = Some(Current { card, front: written.front, back: written.back, answered: false });
                break;
            }
        }
        self.show_review();
    }

    /// The list's column hidden while a card is shown (owner's request: a card alone beside the
    /// decks), and back with the review's end.
    fn show_list_column(&self, shown: bool) {
        if self.list_item.isCollapsed() == shown {
            self.list_item.setCollapsed(!shown);
        }
    }

    fn show_review(&self) {
        let shown = {
            let review = self.review.borrow();
            let Some(review) = review.as_ref() else { return };
            match &review.current {
                None => Shown::Done { deck: review.deck.clone(), reviewed: review.reviewed },
                Some(current) => {
                    let answer = current.answered.then(|| {
                        let state = current.card.id.as_deref().and_then(|id| self.card_state(id));
                        let at = now();
                        let settings = self.deck_settings.borrow();
                        (current.back.clone(), [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy].map(|grade| comes_back(&graded(&settings, &current.card.deck, state, at, grade))))
                    });
                    Shown::Card { deck: review.deck.clone(), done: review.reviewed, left: review.queue.len() + 1, front: current.front.clone(), answer }
                }
            }
        };
        self.show_list_column(!matches!(shown, Shown::Card { .. }));
        self.review_view.show(Some(shown));
    }

    fn card_state(&self, id: &str) -> Option<CardState> {
        let mut state = self.state.borrow_mut();
        self.ensure_reader(&mut state);
        state.reader.as_ref().and_then(|r| r.card_state(id).ok()).flatten()
    }

    /// The card shown graded: given an id in its note if it has none, its review logged in the
    /// vault (this Mac's log), the next card shown. Graded Again, it comes back at the review's
    /// end.
    fn grade_card(&self, grade: Grade) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        let Some(mut card) = self.review.borrow().as_ref().and_then(|r| r.current.as_ref()).filter(|c| c.answered).map(|c| c.card.clone()) else { return };
        if card.id.is_none() {
            // What is typed and not written yet is written first, not written over.
            let _ = self.write_all();
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
            let id = card_id(seed);
            let given = files.read_utf8(&card.path).map_err(|e| e.to_string()).and_then(|text| {
                let (at, mark) = give_id(&text, card.start as usize..card.end as usize, &id).ok_or_else(|| strings::CARD_MOVED.to_owned())?;
                files.write_note(&card.path, &format!("{}{mark}{}", &text[..at], &text[at..])).map_err(|e| e.to_string())
            });
            if let Err(e) = given {
                self.tell_failed(&strings::review_failed(document_name(&card.path)), &e);
                return;
            }
            card.id = Some(id);
        }
        let id = card.id.clone().unwrap_or_default();
        // Where it stood before this review (the index has not taken it yet): whether it is
        // still learned after it, and so asked again in this review.
        let state = self.card_state(&id);
        let learned = graded(&self.deck_settings.borrow(), &card.deck, state, now(), grade).step.is_some();
        let device = knotes_session::AppSettings::device_name(&knotes_session::app_data_dir());
        if let Err(e) = files.append_review(&device, &review_line(now(), &id, grade)) {
            self.tell_failed(&strings::review_failed(document_name(&card.path)), &e);
            return;
        }
        if let Some(review) = self.review.borrow_mut().as_mut() {
            review.reviewed += 1;
            if learned {
                review.queue.push_back(card);
            }
        }
        self.next_card();
    }

    /// The review, for the probe, and whether the list's column shows beside it.
    pub fn describe_review(&self) -> String {
        format!("{}{}", self.review_view.describe(), if self.list_item.isCollapsed() { " (list hidden)" } else { "" })
    }

    /// Clicks in the review, for the probe: `start`, `show`, `1` to `4`, `open`, commas between.
    pub fn probe_review(&self, clicks: &str) {
        for click in clicks.split(',') {
            let asked = match click.trim() {
                "start" => Asked::Start,
                "ahead" => Asked::StartAhead,
                "show" => Asked::ShowAnswer,
                "open" => Asked::OpenNote,
                // `link0`: the first link of the card's text.
                link if link.starts_with("link") => {
                    self.review_view.click_link(link[4..].parse().unwrap_or(0));
                    continue;
                }
                grade => match grade.parse().ok().and_then(Grade::from_number) {
                    Some(grade) => Asked::Grade(grade),
                    None => continue,
                },
            };
            self.review_view.click(asked);
        }
    }
}

/// Every deck of the tree, by its path.
fn deck_paths(decks: &[TagNode]) -> Vec<String> {
    let mut paths = Vec::new();
    fn add(nodes: &[TagNode], paths: &mut Vec<String>) {
        for node in nodes {
            paths.push(node.tag.clone());
            add(&node.children, paths);
        }
    }
    add(decks, &mut paths);
    paths
}
