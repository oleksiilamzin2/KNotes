//! The outline of the document open, for the right panel (ADR 0023): a note's headings or a
//! PDF's bookmarks, nested by level.

/// Where a heading is: in a note, its offset in the text (UTF-16) and 0; in a PDF, its page
/// (index) and how far down the page (a fraction). Places compare in reading order.
pub type OutlinePlace = (usize, f64);

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    /// 1 for the top level.
    pub level: u8,
    pub title: String,
    pub place: OutlinePlace,
}

/// Headings in the document's order, each under the last one above it of a higher level.
#[derive(Debug, Default)]
pub struct Outline {
    entries: Vec<OutlineEntry>,
    parents: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    roots: Vec<usize>,
}

impl Outline {
    pub fn new(entries: Vec<OutlineEntry>) -> Self {
        let mut parents = Vec::with_capacity(entries.len());
        let mut children = vec![Vec::new(); entries.len()];
        let mut roots = Vec::new();
        // The headings a later one can sit under, levels rising.
        let mut open: Vec<usize> = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            while open.last().is_some_and(|&o| entries[o].level >= entry.level) {
                open.pop();
            }
            let parent = open.last().copied();
            match parent {
                Some(p) => children[p].push(i),
                None => roots.push(i),
            }
            parents.push(parent);
            open.push(i);
        }
        Self { entries, parents, children, roots }
    }

    pub fn entries(&self) -> &[OutlineEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The headings right under heading `of`, or the top ones (`None`).
    pub fn children(&self, of: Option<usize>) -> &[usize] {
        match of {
            Some(i) => self.children.get(i).map_or(&[], Vec::as_slice),
            None => &self.roots,
        }
    }

    pub fn parent(&self, i: usize) -> Option<usize> {
        self.parents.get(i).copied().flatten()
    }

    /// The heading of the part at `top` (the top of the view): the last at or above it.
    pub fn current(&self, top: OutlinePlace) -> Option<usize> {
        // The last of equal places (`max_by` keeps the last).
        self.entries.iter().enumerate().filter(|(_, e)| e.place <= top).max_by(|a, b| a.1.place.partial_cmp(&b.1.place).unwrap_or(std::cmp::Ordering::Equal)).map(|(i, _)| i)
    }

    /// Heading `i` known again after the text around it changes: its title and its parents',
    /// from the top.
    pub fn key(&self, i: usize) -> String {
        let mut titles = vec![self.entries[i].title.as_str()];
        let mut at = i;
        while let Some(parent) = self.parent(at) {
            titles.push(&self.entries[parent].title);
            at = parent;
        }
        titles.reverse();
        titles.join("\u{1f}")
    }
}
