//! The notes' cards in the window (ADR 0037): a card made from what is selected in the note,
//! and a deck's review in the note column.

use std::collections::VecDeque;

use knotes_cards::{Fsrs, Grade, Memory, asks, card_id, give_id, make_card, review_line, scan_cards};
use knotes_core::{CardIndex, CardState, DueCard};

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

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// When a card would come back, said short: «10 хв», «3 д», «2 міс», «1,5 р».
fn comes_back(fsrs: &Fsrs, state: Option<CardState>, at: i64, grade: Grade) -> String {
    if grade == Grade::Again {
        return strings::REVIEW_AGAIN_IN.to_owned();
    }
    let days_since = state.map_or(0.0, |s| (at - s.last).max(0) as f64 / DAY);
    let memory = fsrs.review(state.map(|s| Memory { stability: s.stability, difficulty: s.difficulty }), days_since, grade);
    strings::review_interval(fsrs.interval(memory.stability))
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
        self.review_view.show(Some(Shown::Waiting { deck: deck.to_owned(), cards: cards.len(), more: cards.len() == REVIEW_CARDS }));
        self.update_navigation();
    }

    /// The review goes: another list is chosen, or a document opened.
    pub(super) fn hide_review(&self) {
        self.show_list_column(true);
        *self.review.borrow_mut() = None;
        *self.review_deck.borrow_mut() = None;
        self.review_view.show(None);
    }

    fn due_cards(&self, deck: &str) -> Vec<DueCard> {
        let mut state = self.state.borrow_mut();
        self.ensure_reader(&mut state);
        state.reader.as_ref().and_then(|r| r.due_cards(Some(deck), now(), REVIEW_CARDS).ok()).unwrap_or_default()
    }

    /// A button of the review.
    pub(super) fn review_asked(&self, asked: Asked) {
        match asked {
            Asked::Start => {
                let Some(deck) = self.review_deck.borrow().clone() else { return };
                let queue = self.due_cards(&deck).into();
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
                        let (fsrs, at) = (Fsrs::default(), now());
                        (current.back.clone(), [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy].map(|grade| comes_back(&fsrs, state, at, grade)))
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
        let device = knotes_session::AppSettings::device_name(&knotes_session::app_data_dir());
        if let Err(e) = files.append_review(&device, &review_line(now(), &id, grade)) {
            self.tell_failed(&strings::review_failed(document_name(&card.path)), &e);
            return;
        }
        if let Some(review) = self.review.borrow_mut().as_mut() {
            review.reviewed += 1;
            if grade == Grade::Again {
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
