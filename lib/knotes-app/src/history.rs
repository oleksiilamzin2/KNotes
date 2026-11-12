//! Back/forward history of opened notes, as in a browser.

/// Notes remembered behind the current one.
pub const HISTORY_DEPTH: usize = 100;

#[derive(Debug, Default)]
pub struct History {
    back: Vec<String>,
    current: Option<String>,
    forward: Vec<String>,
}

impl History {
    /// Opens a note: the current one goes back, the forward stack is dropped. Visiting the
    /// current note again does nothing.
    pub fn visit(&mut self, path: &str) {
        if self.current.as_deref() == Some(path) {
            return;
        }
        if let Some(current) = self.current.replace(path.to_owned()) {
            self.back.push(current);
            if self.back.len() > HISTORY_DEPTH {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }

    pub fn back(&mut self) -> Option<&str> {
        let previous = self.back.pop()?;
        if let Some(current) = self.current.replace(previous) {
            self.forward.push(current);
        }
        self.current.as_deref()
    }

    pub fn forward(&mut self) -> Option<&str> {
        let next = self.forward.pop()?;
        if let Some(current) = self.current.replace(next) {
            self.back.push(current);
        }
        self.current.as_deref()
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// A note was renamed or moved: every step on it follows.
    pub fn rename(&mut self, from: &str, to: &str) {
        for step in self.back.iter_mut().chain(self.current.iter_mut()).chain(self.forward.iter_mut()) {
            if step == from {
                *step = to.to_owned();
            }
        }
    }

    /// Drops notes that no longer exist; neighbours that became equal merge into one step.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        if self.current.as_deref().is_some_and(|c| !keep(c)) {
            self.current = None;
        }
        for stack in [&mut self.back, &mut self.forward] {
            stack.retain(|p| keep(p));
            stack.dedup();
            if stack.last() == self.current.as_ref() {
                stack.pop();
            }
        }
    }
}
