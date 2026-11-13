//! The documents open as tabs in the sidebar (ADR 0024): a note opens in the tab shown, a PDF
//! (or a note asked to) in a new tab beside it, and each document is shown again where it was.

use std::collections::HashMap;

use serde_json::{Value, json};

/// Where a document was: the place at the top (a note's paragraph offset, UTF-16, and 0; a
/// PDF's page and how far down it) and a note's selection (location, length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabPlace {
    pub top: (usize, f64),
    pub cursor: Option<(usize, usize)>,
}

/// What closing a tab leaves to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterClose {
    /// The tab shown stays.
    Same,
    /// The tab shown was closed: this document's tab is shown now.
    Show(String),
    /// No tab is left.
    Nothing,
}

#[derive(Debug, Default)]
pub struct Tabs {
    /// The documents of the tabs, in order.
    tabs: Vec<String>,
    current: Option<usize>,
    /// Where each document was when last left, tab or not: a tab moving on to another
    /// document does not lose where the first was.
    places: HashMap<String, TabPlace>,
}

impl Tabs {
    /// Shows `path` in the tab shown (a first tab when there is none), or brings back its tab
    /// when it has one; gives where it was.
    pub fn navigate(&mut self, path: &str) -> Option<TabPlace> {
        match (self.position(path), self.current) {
            (Some(i), _) => self.current = Some(i),
            (None, Some(c)) => self.tabs[c] = path.to_owned(),
            (None, None) => {
                self.tabs.push(path.to_owned());
                self.current = Some(self.tabs.len() - 1);
            }
        }
        self.places.get(path).copied()
    }

    /// Shows `path` in a new tab right after the one shown, or brings back its tab when it has
    /// one; gives where it was.
    pub fn open_beside(&mut self, path: &str) -> Option<TabPlace> {
        let i = self.position(path).unwrap_or_else(|| {
            let at = self.current.map_or(self.tabs.len(), |c| c + 1);
            self.tabs.insert(at, path.to_owned());
            at
        });
        self.current = Some(i);
        self.places.get(path).copied()
    }

    fn position(&self, path: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t == path)
    }

    /// Where the document of the tab shown is now.
    pub fn remember(&mut self, place: TabPlace) {
        if let Some(path) = self.current_path().map(str::to_owned) {
            self.places.insert(path, place);
        }
    }

    /// Closes tab `i`.
    pub fn close(&mut self, i: usize) -> AfterClose {
        if i >= self.tabs.len() {
            return AfterClose::Same;
        }
        self.tabs.remove(i);
        match self.current {
            Some(c) if c == i => {
                if self.tabs.is_empty() {
                    self.current = None;
                    return AfterClose::Nothing;
                }
                let next = i.min(self.tabs.len() - 1);
                self.current = Some(next);
                AfterClose::Show(self.tabs[next].clone())
            }
            Some(c) if c > i => {
                self.current = Some(c - 1);
                AfterClose::Same
            }
            _ => AfterClose::Same,
        }
    }

    /// Closes every tab but `i`, which shows.
    pub fn close_others(&mut self, i: usize) -> AfterClose {
        if i >= self.tabs.len() {
            return AfterClose::Same;
        }
        let kept = self.tabs.swap_remove(i);
        let shown = self.current == Some(i);
        self.tabs = vec![kept];
        self.current = Some(0);
        if shown { AfterClose::Same } else { AfterClose::Show(self.tabs[0].clone()) }
    }

    /// A document moved from `from` to `to`.
    pub fn rename(&mut self, from: &str, to: &str) {
        for tab in self.tabs.iter_mut().filter(|t| *t == from) {
            *tab = to.to_owned();
        }
        if let Some(place) = self.places.remove(from) {
            self.places.insert(to.to_owned(), place);
        }
    }

    /// Keeps the tabs of documents `exists` says are still there; with the one shown gone,
    /// none is shown.
    pub fn retain(&mut self, exists: impl Fn(&str) -> bool) {
        let current = self.current_path().map(str::to_owned);
        self.tabs.retain(|t| exists(t));
        self.current = current.and_then(|c| self.position(&c));
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.tabs.iter().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// The tab shown (its index).
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn current_path(&self) -> Option<&str> {
        self.current.and_then(|i| self.tabs.get(i)).map(String::as_str)
    }

    /// Where `path`'s document was.
    pub fn place_of(&self, path: &str) -> Option<TabPlace> {
        self.places.get(path).copied()
    }

    /// The tabs, the places of their documents and the tab shown.
    pub fn to_json(&self) -> Value {
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .map(|path| match self.places.get(path) {
                Some(p) => json!({ "path": path, "top": [p.top.0, p.top.1], "cursor": p.cursor.map(|c| [c.0, c.1]) }),
                None => json!({ "path": path }),
            })
            .collect();
        json!({ "tabs": tabs, "current": self.current })
    }

    /// Tabs read back from the text `to_json` gave; anything broken gives none.
    pub fn from_text(text: &str) -> Self {
        serde_json::from_str(text).map(|value| Self::from_json(&value)).unwrap_or_default()
    }

    /// Tabs read back; anything broken gives none.
    pub fn from_json(value: &Value) -> Self {
        let mut tabs = Self::default();
        for t in value.get("tabs").and_then(Value::as_array).into_iter().flatten() {
            let Some(path) = t.get("path").and_then(Value::as_str) else { continue };
            let top = t.get("top").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_f64()?)));
            let cursor = t.get("cursor").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
            tabs.tabs.push(path.to_owned());
            if let Some(top) = top {
                tabs.places.insert(path.to_owned(), TabPlace { top, cursor });
            }
        }
        tabs.current = value.get("current").and_then(Value::as_u64).map(|c| c as usize).filter(|&c| c < tabs.tabs.len());
        tabs
    }
}
