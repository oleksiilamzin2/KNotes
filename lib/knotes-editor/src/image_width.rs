//! An image embed's written width (ADR 0020).

/// `source`, an image embed (`![[pic.png|…]]` or `![alt|…](pic.png)`), with `width` as its
/// size: in place of the size it had (`200` or `200x100`, the height then following the
/// width), or after its alt text.
pub fn set_image_width(source: &str, width: u32) -> String {
    let wiki = source.starts_with("![[");
    let (start, end) = if wiki { (3, source.len().saturating_sub(2)) } else { (2, source.find("](").unwrap_or(2)) };
    let Some(text) = source.get(start..end) else { return source.to_owned() };
    let new = match text.rsplit_once('|') {
        Some((rest, size)) if is_size(size) => format!("{rest}|{width}"),
        _ if !wiki && is_size(text) => width.to_string(),
        _ => format!("{text}|{width}"),
    };
    format!("{}{new}{}", &source[..start], &source[end..])
}

/// `300` or `300x200`.
fn is_size(text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let text = text.trim();
    text.split_once('x').map_or(digits(text), |(w, h)| digits(w) && digits(h))
}
