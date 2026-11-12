use std::collections::HashSet;

use rand::RngExt;

use crate::Rng;
use crate::word_source::WordSource;
use crate::zipf::Zipf;

/// Real frequent words first, then generated pseudo-words up to `size`, drawn with Zipf frequency.
/// Ukrainian pseudo-words are stems with several case endings, so word forms repeat as in real text.
pub(crate) struct Vocabulary {
    words: Vec<String>,
    zipf: Zipf,
}

const UK_BASE: &str = "і в на що з не до як це для за про від але або так вже також після коли дуже можна треба \
тому який яка яке які цей ця ці свій може було був була буде є має мають щоб якщо тільки ще все всі один два три \
перший другий новий великий малий інший кожен багато мало потім тут там де чому бо хоча навіть система дані робота \
проєкт радар сигнал частота модель мережа пошук нотатка база знання файл тека графік таблиця формула документ \
рішення питання відповідь команда сервер клієнт запит індекс швидкість пам'ять процесор екран вікно кнопка текст \
рядок слово мова код тест помилка версія оновлення налаштування користувач завдання план звіт зустріч день тиждень \
місяць рік час місто країна людина книга стаття джерело приклад метод алгоритм структура функція значення результат \
аналіз експеримент вимірювання антена фільтр спектр шум потужність енергія струм напруга температура тиск відстань \
швидко повільно точно добре погано важливо цікаво складно просто";

const EN_BASE: &str = "the of and to in is for on with as that this by from at are be it or an was were will can \
has have not but all any each more most other some such only also than then so if when where which who how what why \
data system model search note index file folder graph table query server client signal frequency radar network memory \
speed test error version update user task plan report meeting day week month year time city country person book \
article source example method algorithm structure function value result analysis experiment measurement antenna \
filter spectrum noise power energy current voltage temperature pressure distance fast slow exact good bad important \
interesting complex simple design review draft idea project team work issue release build deploy cache store write \
read open close start stop learn teach research paper chapter section summary detail";

const UK_CONSONANTS: [&str; 19] = ["б", "в", "г", "д", "ж", "з", "к", "л", "м", "н", "п", "р", "с", "т", "ф", "х", "ц", "ч", "ш"];
const UK_VOWELS: [&str; 9] = ["а", "е", "и", "і", "о", "у", "я", "ю", "є"];
const UK_ENDINGS: [&str; 16] = ["", "а", "и", "і", "у", "ою", "ами", "ах", "ів", "ові", "ом", "ий", "е", "ого", "ому", "ими"];
const EN_CONSONANTS: [&str; 16] = ["b", "c", "d", "f", "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w"];
const EN_VOWELS: [&str; 5] = ["a", "e", "i", "o", "u"];
const EN_ENDINGS: [&str; 6] = ["", "s", "ed", "ing", "er", "ly"];

/// Function words at the top of the base lists are not used in titles and tags.
const FUNCTION_WORDS: usize = 40;
/// Content words for titles are drawn uniformly from this many top ranks.
const CONTENT_RANKS: usize = 4000;

impl Vocabulary {
    pub fn ukrainian(size: usize, rng: &mut Rng) -> Self {
        Self::build(UK_BASE, &UK_CONSONANTS, &UK_VOWELS, &UK_ENDINGS, size, rng)
    }

    pub fn english(size: usize, rng: &mut Rng) -> Self {
        Self::build(EN_BASE, &EN_CONSONANTS, &EN_VOWELS, &EN_ENDINGS, size, rng)
    }

    fn build(base: &str, consonants: &[&str], vowels: &[&str], endings: &[&str], size: usize, rng: &mut Rng) -> Self {
        let mut words: Vec<String> = base.split_whitespace().map(str::to_owned).collect();
        let mut seen: HashSet<String> = words.iter().cloned().collect();
        while words.len() < size {
            let mut stem = String::new();
            for _ in 0..rng.random_range(2..=3) {
                stem.push_str(consonants[rng.random_range(0..consonants.len())]);
                stem.push_str(vowels[rng.random_range(0..vowels.len())]);
            }
            stem.push_str(consonants[rng.random_range(0..consonants.len())]);
            for _ in 0..rng.random_range(1..=4) {
                let word = format!("{stem}{}", endings[rng.random_range(0..endings.len())]);
                if words.len() < size && seen.insert(word.clone()) {
                    words.push(word);
                }
            }
        }
        let zipf = Zipf::new(words.len(), 1.0, 2.7);
        Self { words, zipf }
    }
}

impl WordSource for Vocabulary {
    fn word(&self, rng: &mut Rng) -> &str {
        &self.words[self.zipf.sample(rng)]
    }

    fn content_word(&self, rng: &mut Rng) -> &str {
        let top = CONTENT_RANKS.min(self.words.len());
        &self.words[rng.random_range(FUNCTION_WORDS.min(top - 1)..top)]
    }
}
