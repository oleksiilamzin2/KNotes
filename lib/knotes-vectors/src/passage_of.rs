use knotes_core::BlockHit;
use knotes_embed::Passage;

/// What `block` of the document at `path` (its text `text`) is embedded as: its text with the
/// document's title, and its section's heading before it unless the text starts with it.
pub fn block_passage(path: &str, text: &str, block: &BlockHit) -> Passage {
    let body = text.get(block.span.start as usize..block.span.end as usize).unwrap_or_default().trim();
    let text = match &block.heading {
        Some(heading) if !body.lines().next().is_some_and(|first| first.contains(heading.as_str())) => format!("{heading} › {body}"),
        _ => body.to_string(),
    };
    Passage::Document { title: title_of(path), text }
}

/// A document's title: its file name without the extension.
fn title_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}
