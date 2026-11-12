//! Suffix stripping for Ukrainian and English query words. Deliberately light: the stem is
//! searched as a prefix, so it only has to cut inflection endings, not find the true root.

use crate::stemmer::Stemmer;

/// The stem keeps at least this many characters; shorter words are searched exactly.
const MIN_STEM: usize = 3;

/// The last token of a stem (after an apostrophe or hyphen) keeps at least this many characters.
const MIN_LAST_TOKEN: usize = 2;

fn last_token_chars(stem: &str) -> usize {
    stem.rsplit(|c: char| !c.is_alphanumeric()).next().map_or(0, |t| t.chars().count())
}

/// Longest first, so `ами` wins over `и`.
const UK_ENDINGS: &[&str] = &[
    "ування", "ювання", "ючись", "учись", "ються", "уються", "ували", "ювали", "ями", "ами", "ові", "еві", "єві",
    "ого", "ому", "ими", "іми", "ють", "ують", "ать", "ять", "ить", "емо", "ємо", "имо", "ете", "єте", "ите", "уть", "ала",
    "ало", "али", "ила", "ило", "или", "ою", "ею", "єю", "ах", "ях", "ів", "їв", "ей", "ий", "ій", "ої", "им", "их", "ім",
    "іх", "ом", "ем", "єм", "ти", "ть", "ся", "сь", "ла", "ло", "ли", "а", "я", "о", "е", "є", "и", "і", "ї", "у", "ю", "й",
    "ь",
];

const EN_ENDINGS: &[&str] = &["ations", "ation", "ings", "ing", "ness", "ment", "ies", "ied", "ed", "es", "ly", "er", "est", "s"];

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LightStemmer;

impl Stemmer for LightStemmer {
    fn stem<'a>(&self, word: &'a str) -> &'a str {
        let cyrillic = word.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c));
        let endings = if cyrillic { UK_ENDINGS } else { EN_ENDINGS };
        let chars = word.chars().count();
        endings
            .iter()
            .filter(|e| word.ends_with(*e) && chars - e.chars().count() >= MIN_STEM)
            // FTS5 applies the prefix to the last token of the stem: `пам'` (from `пам'ять`) would
            // lose it, and `пам'я` would expand `я*` to thousands of terms (20 ms per query).
            .filter(|e| last_token_chars(&word[..word.len() - e.len()]) >= MIN_LAST_TOKEN)
            .filter(|&&e| e != "es" || ["s", "x", "z", "ch", "sh"].iter().any(|p| word[..word.len() - 2].ends_with(p)))
            .max_by_key(|e| e.len())
            .map_or(word, |e| &word[..word.len() - e.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stem(w: &str) -> &str {
        LightStemmer.stem(w)
    }

    #[test]
    fn ukrainian_forms_share_a_stem() {
        for w in ["мова", "мови", "мові", "мовою"] {
            assert_eq!(stem(w), "мов", "{w}");
        }
        for w in ["радар", "радара", "радарами", "радарах"] {
            assert_eq!(stem(w), "радар", "{w}");
        }
        assert_eq!(stem("роботою"), "робот");
        assert_eq!(stem("знаннями"), "знанн");
        assert_eq!(stem("пам'яттю"), "пам'ятт");
        assert_eq!(stem("пам'ять"), "пам'ят");
    }

    #[test]
    fn english_forms_share_a_stem() {
        for w in ["search", "searches", "searched", "searching"] {
            assert_eq!(stem(w), "search", "{w}");
        }
        assert_eq!(stem("notes"), "note");
        assert_eq!(stem("quickly"), "quick");
    }

    #[test]
    fn short_words_are_kept() {
        for w in ["і", "та", "мої", "the", "is"] {
            assert_eq!(stem(w), w);
        }
    }
}
