//! Finder-like ordering: case-insensitive, digit runs compared as numbers ("2" < "10").

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let (n, m) = (digits(&mut x), digits(&mut y));
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let order = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                x.next();
                y.next();
                let order = c.to_lowercase().cmp(d.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

fn digits(chars: &mut Peekable<Chars>) -> String {
    let mut s = String::new();
    while let Some(c) = chars.next_if(char::is_ascii_digit) {
        s.push(c);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering::*;

    #[test]
    fn numbers_and_case() {
        assert_eq!(natural_cmp("Folder 2", "Folder 10"), Less);
        assert_eq!(natural_cmp("folder 2", "Folder 10"), Less);
        assert_eq!(natural_cmp("a07", "a7"), Less, "equal numbers fall back to exact order");
        assert_eq!(natural_cmp("abc", "ABD"), Less);
        assert_eq!(natural_cmp("Архів", "Біологія"), Less);
    }
}
