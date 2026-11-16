//! What is kept from the AI (ADR 0036): a note by its `#no-ai` tag, another file or a folder by
//! its folder's rules file. The list's and the sidebar's menus keep a row's document or folder
//! from the AI or show it again; the snapshot that follows the write marks it.

use knotes_editor::{with_tag, without_tag};
use std::collections::BTreeMap;

use knotes_spaces::{KeptFromAi, NO_AI_TAG, file_ai, set_file_ai, set_folder_ai};

use super::*;

/// The folder `path` is in (`""`: the vault's root).
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// A folder's rules file, as the user calls it.
fn rules_file(folder: &str) -> String {
    if folder.is_empty() { knotes_app::RULES_FILE.to_owned() } else { format!("{folder}/{}", knotes_app::RULES_FILE) }
}

/// `folder`'s rules text in `texts` (as read, and as it becomes), read the first time.
fn rules_text<'t>(texts: &'t mut BTreeMap<String, (String, String)>, files: &FsVault, folder: &str) -> Result<&'t mut String, String> {
    if !texts.contains_key(folder) {
        let text = files.read_rules(&[folder]).map_err(|e| e.to_string())?.into_iter().next().map(|(_, text)| text).unwrap_or_default();
        texts.insert(folder.to_owned(), (text.clone(), text));
    }
    Ok(&mut texts.get_mut(folder).expect("just read").1)
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// What a file's or a folder's own setting must be for it to be kept (`keep`) or shown, its
/// folder keeping it or not: none where the folder's does it already.
fn own_setting(keep: bool, folder_keeps: bool) -> Option<bool> {
    (keep != folder_keeps).then_some(!keep)
}

impl VaultWindow {
    /// What is kept from the AI now, as the last snapshot read it.
    fn kept_from_ai(&self) -> Arc<KeptFromAi> {
        Arc::clone(&self.state.borrow().ai)
    }

    /// A list row's menu: its document kept from the AI, or shown to it. A note is tagged
    /// `#no-ai`, or loses the tag (and its folder, when that keeps it, is told the note is
    /// shared); another file is named in its folder's rules file where that differs from its
    /// folder's setting.
    pub(super) fn toggle_ai(&self, path: &str) {
        let ai = self.kept_from_ai();
        let keep = !ai.holds(path);
        let folder_keeps = ai.holds_folder(parent(path));
        let done = if FileKind::from_path(path) == FileKind::Markdown {
            let tagged = if keep || ai.tagged(path) { self.retag(path, keep) } else { Ok(()) };
            let wanted = folder_keeps.then_some(true);
            tagged.and_then(|()| if keep || ai.own_file(path) == wanted { Ok(()) } else { self.set_own_file(path, wanted) })
        } else {
            let wanted = own_setting(keep, folder_keeps);
            if ai.own_file(path) == wanted { Ok(()) } else { self.set_own_file(path, wanted) }
        };
        if let Err(e) = done {
            self.tell_failed(&strings::ai_failed(document_name(path), keep), &e);
        }
    }

    /// A folder's menu: kept from the AI with what is in it, or shown to it; its rules file says
    /// so where that differs from its parent's setting.
    pub(super) fn toggle_folder_ai(&self, folder: &str) {
        let ai = self.kept_from_ai();
        let keep = !ai.holds_folder(folder);
        let wanted = own_setting(keep, ai.holds_folder(parent(folder)));
        if ai.own_folder(folder) == wanted {
            return;
        }
        let done = self.edit_rules(folder, |text| set_folder_ai(text, wanted));
        if let Err(e) = done {
            let name = folder.rsplit('/').next().unwrap_or(folder);
            let file = if folder.is_empty() { knotes_app::RULES_FILE.to_owned() } else { format!("{folder}/{}", knotes_app::RULES_FILE) };
            self.tell_failed(&strings::ai_failed(name, keep), &strings::failed_files(&e, &[&file]));
        }
    }

    /// The rules files, by folder, as they are once each file `moves` takes from one path to
    /// another (a rename, a move, a restore under another name) carries its own setting for the
    /// AI with it, as a note carries its tag: into its new folder's rules, out of its old one's
    /// (kept there when `copy`). Only those that change. Else the file kept from the AI whose
    /// new folder's rules cannot say so (unreadable, or not parsing), and why.
    pub(super) fn carried_ai<'a>(&self, moves: impl IntoIterator<Item = (&'a str, &'a str)>, copy: bool) -> Result<Vec<(String, String)>, (String, String)> {
        let Some(vault) = self.vault() else { return Ok(Vec::new()) };
        let files = FsVault::new(&vault);
        // Each folder's rules as read, and as they become.
        let mut texts: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (from, to) in moves {
            // Rules that cannot be read or parsed say nothing of the file.
            let Some(own) = rules_text(&mut texts, &files, parent(from)).ok().and_then(|text| file_ai(text, file_name(from))) else { continue };
            let carried = rules_text(&mut texts, &files, parent(to)).and_then(|text| set_file_ai(text, file_name(to), Some(own)).map(|edited| *text = edited));
            match carried {
                Ok(()) => {}
                // Shared by its own setting: where it goes decides.
                Err(_) if own => continue,
                Err(e) => return Err((from.to_owned(), strings::ai_rules_unwritable(&rules_file(parent(to)), &e))),
            }
            if !copy && from != to && let Ok(text) = rules_text(&mut texts, &files, parent(from)) {
                *text = set_file_ai(text, file_name(from), None).unwrap_or_else(|_| text.clone());
            }
        }
        Ok(texts.into_iter().filter(|(_, (was, now))| was != now).map(|(folder, (_, now))| (folder, now)).collect())
    }

    /// Carries the moved files' own settings for the AI (`carried_ai`), saying so when a rules
    /// file cannot be written: what it should keep from the AI is no longer kept.
    pub(super) fn carry_ai<'a>(&self, moves: impl IntoIterator<Item = (&'a str, &'a str)> + Clone, copy: bool) {
        let Some(vault) = self.vault() else { return };
        let files = FsVault::new(&vault);
        let failed = match self.carried_ai(moves.clone(), copy) {
            Ok(texts) => texts.into_iter().find_map(|(folder, text)| files.write_rules(&folder, &text).err().map(|e| (rules_file(&folder), e.to_string()))),
            Err((_, why)) => Some((String::new(), why)),
        };
        if let Some((file, why)) = failed {
            let names: Vec<&str> = moves.into_iter().map(|(_, to)| document_name(to)).collect();
            let why = if file.is_empty() { why } else { strings::failed_files(&why, &[&file]) };
            self.tell_failed(&strings::ai_not_carried(&names.join(", ")), &why);
        }
    }

    /// The file `path`'s own `ai` in its folder's rules file: `wanted`, none to take it out.
    fn set_own_file(&self, path: &str, wanted: Option<bool>) -> Result<(), String> {
        let name = path.rsplit('/').next().unwrap_or(path);
        let folder = parent(path);
        self.edit_rules(folder, |text| set_file_ai(text, name, wanted)).map_err(|e| {
            let file = if folder.is_empty() { knotes_app::RULES_FILE.to_owned() } else { format!("{folder}/{}", knotes_app::RULES_FILE) };
            strings::failed_files(&e, &[&file])
        })
    }

    /// `folder`'s rules file rewritten by `edit`: one that cannot be read, or does not parse, is
    /// not written over.
    fn edit_rules(&self, folder: &str, edit: impl FnOnce(&str) -> Result<String, String>) -> Result<(), String> {
        let Some(vault) = self.vault() else { return Ok(()) };
        let files = FsVault::new(&vault);
        let text = files.read_rules(&[folder]).map_err(|e| e.to_string())?.into_iter().next().map(|(_, text)| text).unwrap_or_default();
        let edited = edit(&text)?;
        files.write_rules(folder, &edited).map_err(|e| e.to_string())
    }

    /// The note at `path` tagged `#no-ai` (`keep`) or not. The note open is edited in the editor,
    /// as typed (it can be undone), and saved; another is rewritten on the disk.
    fn retag(&self, path: &str, keep: bool) -> Result<(), String> {
        let change = |text: &str| if keep { with_tag(text, NO_AI_TAG) } else { without_tag(text, NO_AI_TAG) };
        if self.open_path().as_deref() == Some(path) {
            if self.editing.borrow().as_ref().is_some_and(|e| e.read_only) {
                return Err(strings::READ_ONLY_TAG.to_owned());
            }
            let old = self.note_text.string().to_string();
            let new = change(&old);
            if new != old {
                replace_changed(&self.note_text, &old, &new);
                return self.write_all().map_err(|(_, e)| e.to_string());
            }
            return Ok(());
        }
        // Edits typed in an embedded copy of it written first, not written over.
        self.write_all().map_err(|(_, e)| e.to_string())?;
        let Some(vault) = self.vault() else { return Ok(()) };
        let files = FsVault::new(&vault);
        let old = files.read_utf8(path).map_err(|e| e.to_string())?;
        let new = change(&old);
        if new == old {
            return Ok(());
        }
        files.write_note(path, &new).map_err(|e| e.to_string())
    }
}

/// `view`'s text from `old` to `new` by the part between their common start and end, as an edit
/// that can be undone; the selection and the scroll stay where they were.
fn replace_changed(view: &NSTextView, old: &str, new: &str) {
    let start = old.char_indices().zip(new.chars()).take_while(|((_, a), b)| a == b).last().map_or(0, |((i, c), _)| i + c.len_utf8());
    let (old_rest, new_rest) = (&old[start..], &new[start..]);
    let end = old_rest.chars().rev().zip(new_rest.chars().rev()).take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum::<usize>();
    let end = end.min(old_rest.len()).min(new_rest.len());
    let from = old[..start].encode_utf16().count();
    let range = NSRange::new(from, old_rest[..old_rest.len() - end].encode_utf16().count());
    let replacement = NSString::from_str(&new_rest[..new_rest.len() - end]);
    let selection = view.selectedRange();
    if view.shouldChangeTextInRange_replacementString(range, Some(&replacement)) {
        // SAFETY: the editor's own text storage.
        if let Some(storage) = unsafe { view.textStorage() } {
            storage.replaceCharactersInRange_withString(range, &replacement);
        }
        view.didChangeText();
        // Moved with the text after the edit, kept before it.
        let grown = |at: usize| if at >= range.location + range.length { (at + replacement.length()).saturating_sub(range.length) } else { at.min(range.location + replacement.length()) };
        view.setSelectedRange(NSRange::new(grown(selection.location), 0));
    }
}
