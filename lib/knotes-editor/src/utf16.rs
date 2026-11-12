//! UTF-16 offsets (what `NSString` counts) against byte offsets of Rust strings. Byte scans:
//! a UTF-8 lead byte starts a character, and only 4-byte characters take two UTF-16 units.

#[inline]
fn units_of_lead(b: u8) -> usize {
    match b {
        0x80..=0xBF => 0,
        0xF0.. => 2,
        _ => 1,
    }
}

/// UTF-16 offsets of ascending byte offsets into `source`, in one pass.
pub fn utf16_offsets(source: &str, bytes: &[usize]) -> Vec<usize> {
    debug_assert!(bytes.is_sorted(), "byte offsets must be ascending");
    let raw = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let (mut units, mut at) = (0, 0);
    for &target in bytes {
        let target = target.min(raw.len());
        units += raw[at.min(target)..target].iter().map(|&b| units_of_lead(b)).sum::<usize>();
        at = at.max(target);
        out.push(units);
    }
    out
}

/// Byte offset of the character at UTF-16 offset `units` (clamped to the end).
pub fn byte_offset(source: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, &b) in source.as_bytes().iter().enumerate() {
        let n = units_of_lead(b);
        if n > 0 {
            if seen >= units {
                return i;
            }
            seen += n;
        }
    }
    source.len()
}
