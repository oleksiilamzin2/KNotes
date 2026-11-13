//! The documents open as tabs in the sidebar (ADR 0024): every document opened is a tab, and
//! each remembers where its document was.

use serde_json::{Value, json};

/// Where a document was: the place at the top (a note's paragraph offset, UTF-16, and 0; a
/// PDF's page and how far down it) and a note's selection (location, length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabPlace {
    pub top: (usize, f64),
    pub cursor: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
struct Tab {
    path: String,
    place: Option<TabPlace>,
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
    tabs: Vec<Tab>,
    current: Option<usize>,
}

impl Tabs {
    /// Shows `path`'s tab, a new one last when it has none; gives where it was.
    pub fn open(&mut self, path: &str) -> Option<TabPlace> {
        let i = match self.tabs.iter().position(|t| t.path == path) {
            Some(i) => i,
            None => {
                self.tabs.push(Tab { path: path.to_owned(), place: None });
                self.tabs.len() - 1
            }
        };
        self.current = Some(i);
        self.tabs[i].place
    }

    /// Where the document of the tab shown is now.
    pub fn remember(&mut self, place: TabPlace) {
        if let Some(tab) = self.current.and_then(|i| self.tabs.get_mut(i)) {
            tab.place = Some(place);
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
                AfterClose::Show(self.tabs[next].path.clone())
            }
            Some(c) if c > i => {
                self.current = Some(c - 1);
                AfterClose::Same
            }
            _ => AfterClose::Same,
        }
    }

    /// A document moved from `from` to `to`.
    pub fn rename(&mut self, from: &str, to: &str) {
        for tab in self.tabs.iter_mut().filter(|t| t.path == from) {
            tab.path = to.to_owned();
        }
    }

    /// Keeps the tabs of documents `exists` says are still there; with the one shown gone,
    /// none is shown.
    pub fn retain(&mut self, exists: impl Fn(&str) -> bool) {
        let current = self.current_path().map(str::to_owned);
        self.tabs.retain(|t| exists(&t.path));
        self.current = current.and_then(|c| self.tabs.iter().position(|t| t.path == c));
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.tabs.iter().map(|t| t.path.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// The tab shown (its index).
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn current_path(&self) -> Option<&str> {
        self.current.and_then(|i| self.tabs.get(i)).map(|t| t.path.as_str())
    }

    /// Where `path`'s document was.
    pub fn place_of(&self, path: &str) -> Option<TabPlace> {
        self.tabs.iter().find(|t| t.path == path).and_then(|t| t.place)
    }

    pub fn to_json(&self) -> Value {
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .map(|t| match t.place {
                Some(p) => json!({ "path": t.path, "top": [p.top.0, p.top.1], "cursor": p.cursor.map(|c| [c.0, c.1]) }),
                None => json!({ "path": t.path }),
            })
            .collect();
        json!({ "tabs": tabs, "current": self.current })
    }

    /// Tabs read back; anything broken gives none.
    pub fn from_json(value: &Value) -> Self {
        let tabs: Vec<Tab> = value
            .get("tabs")
            .and_then(Value::as_array)
            .map(|tabs| {
                tabs.iter()
                    .filter_map(|t| {
                        let path = t.get("path")?.as_str()?.to_owned();
                        let top = t.get("top").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_f64()?)));
                        let cursor = t.get("cursor").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
                        Some(Tab { path, place: top.map(|top| TabPlace { top, cursor }) })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let current = value.get("current").and_then(Value::as_u64).map(|c| c as usize).filter(|&c| c < tabs.len());
        Self { tabs, current }
    }
}
