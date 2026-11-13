//! Embeds for images pasted or dropped into a note (ADR 0020).

/// The name Obsidian gives a pasted image, `stamp` being the local time as `yyyyMMddHHmmss`.
pub fn pasted_image_name(stamp: &str) -> String {
    format!("Pasted image {stamp}.png")
}

/// Text embedding the images at vault `paths` where the cursor is, each on a line of its own
/// (an image shows only alone on its line): a line break first when the cursor's line has
/// text `before` the cursor, and last when it has text `after` it. `bare(path)` says whether
/// the file name alone opens that image; otherwise the embed names its path.
pub fn embed_images(paths: &[String], before: &str, after: &str, bare: impl Fn(&str) -> bool) -> String {
    let embeds: Vec<String> = paths
        .iter()
        .map(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            format!("![[{}]]", if bare(path) { name } else { path })
        })
        .collect();
    let lead = if before.trim().is_empty() { "" } else { "\n" };
    let tail = if after.trim().is_empty() { "" } else { "\n" };
    format!("{lead}{}{tail}", embeds.join("\n"))
}
