//! `- []` typed for a task (ADR 0018): Markdown and Obsidian need `- [ ]`, with the space.

/// When `line` (from its start up to a space just typed) is a list item's marker followed by
/// `[] `, the byte offset of the `[]` to make `[ ]`.
pub fn task_box_typed(line: &str) -> Option<usize> {
    let marker = line.strip_suffix("[] ")?;
    let indent = marker.len() - marker.trim_start_matches([' ', '\t']).len();
    let bullet = marker[indent..].strip_suffix(' ')?;
    let digits = bullet.len().saturating_sub(1);
    let is_marker = matches!(bullet, "-" | "*" | "+")
        || (digits > 0 && bullet[..digits].bytes().all(|b| b.is_ascii_digit()) && matches!(&bullet[digits..], "." | ")"));
    is_marker.then_some(marker.len())
}
