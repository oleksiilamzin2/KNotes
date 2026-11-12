/// UTF-16 offsets (what `NSString` counts) of ascending byte offsets into `source`, in one pass.
pub fn utf16_offsets(source: &str, bytes: &[usize]) -> Vec<usize> {
    debug_assert!(bytes.is_sorted(), "byte offsets must be ascending");
    let mut out = Vec::with_capacity(bytes.len());
    let (mut units, mut chars) = (0, source.char_indices().peekable());
    for &target in bytes {
        while let Some(&(at, c)) = chars.peek() {
            if at >= target {
                break;
            }
            units += c.len_utf16();
            chars.next();
        }
        out.push(units);
    }
    out
}
