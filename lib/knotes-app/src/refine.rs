//! What a search's words are narrowed by, written in the query itself: a phrase in quotes, a
//! word that must not be there (`-word`), a tag (`tag:work`), a folder (`folder:Авто`) and a
//! space (`space:Дім`), with `AND` and `OR` between these.

/// Where a document must be: under a tag, in a folder, in a space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A tag the note has (or one under it), without `#`, in small letters.
    Tag(String),
    /// One of its path's folders, or several in a row (`Проєкти/Радар`).
    Folder(String),
    /// Its space, by the space's name.
    Space(String),
}

/// A query taken apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refine {
    /// The words to search by, as typed, the phrases' words among them (a phrase's words must
    /// be found before the phrase can be looked for).
    pub words: String,
    /// The same words as they are asked for: one of each list (`radar OR budget` is a list of
    /// two, `radar budget` two lists of one; a phrase is one word of its own list).
    pub any: Vec<Vec<String>>,
    /// What must stand in the text word for word.
    pub phrases: Vec<String>,
    /// Words the name and the text must not hold.
    pub without: Vec<String>,
    /// Where the document must be: in one place of each list. `tag:a tag:b` (or `tag:a AND
    /// tag:b`) is two lists of one, `space:Дім OR space:Робота` one list of two.
    pub places: Vec<Vec<Place>>,
}

impl Refine {
    /// `query` taken apart. Keys are `tag:`, `folder:` and `space:` (`тег:`, `тека:`, `простір:`
    /// too); a value with spaces goes in quotes (`folder:"Мої проєкти"`). Places narrow
    /// together, with `AND` between them or nothing; `OR` between two makes them one or the
    /// other, of any kinds (`tag:work OR folder:Авто`), and so between two plain words
    /// (`radar OR budget план`: the plan, with either). `AND` elsewhere is left out, as the
    /// words are all asked for anyway; `OR` between anything else (a phrase, a word left out,
    /// a word and a place), a `-` alone and a key with no value are words as any other.
    pub fn parse(query: &str) -> Self {
        let mut refine = Self::default();
        let tokens = tokens(query);
        // A plain word: not a phrase, a place, a word left out, nor `AND` or `OR`.
        let plain = |(token, quoted): &(String, bool)| !quoted && place(token).is_none() && token != "AND" && token != "OR" && !(token.starts_with('-') && token.len() > 1);
        // Whether the piece before was a place, and whether an `OR` joins the next to it.
        let (mut after_place, mut joins) = (false, false);
        // The same of a plain word.
        let (mut after_word, mut joins_word) = (false, false);
        for (i, (token, quoted)) in tokens.iter().enumerate() {
            let named = if *quoted { None } else { place(token) };
            let was_after_place = std::mem::replace(&mut after_place, named.is_some());
            let was_after_word = std::mem::replace(&mut after_word, plain(&tokens[i]));
            if *quoted {
                refine.any.push(vec![token.clone()]);
                refine.phrases.push(token.clone());
            } else if let Some(place) = named {
                match refine.places.last_mut().filter(|_| std::mem::take(&mut joins)) {
                    Some(any) => any.push(place),
                    None => refine.places.push(vec![place]),
                }
            } else if token == "AND" {
                (after_place, after_word) = (was_after_place, was_after_word);
            } else if token == "OR" && was_after_word && tokens.get(i + 1).is_some_and(plain) {
                joins_word = true;
            } else if token == "OR" && was_after_place && tokens.get(i + 1).is_some_and(|(next, quoted)| !quoted && place(next).is_some()) {
                joins = true;
            } else if let Some(word) = token.strip_prefix('-').filter(|word| !word.is_empty()) {
                refine.without.push(word.trim_matches('"').to_owned());
            } else {
                match refine.any.last_mut().filter(|_| std::mem::take(&mut joins_word)) {
                    Some(any) => any.push(token.clone()),
                    None => refine.any.push(vec![token.clone()]),
                }
            }
        }
        refine.words = refine.any.iter().flatten().cloned().collect::<Vec<_>>().join(" ");
        refine
    }

    /// The searches that together find what is asked: each with one word of every list
    /// (`a OR b c`: «a c» and «b c»); one, empty, when no words are typed.
    pub fn queries(&self) -> Vec<String> {
        // shortcut: 16 searches at most, the rest of the alternatives unsearched; ask the
        // index for `OR` itself if queries with five `OR`s are ever typed.
        const MOST: usize = 16;
        let mut queries = vec![String::new()];
        for any in &self.any {
            queries = queries.iter().flat_map(|query| any.iter().map(move |word| if query.is_empty() { word.clone() } else { format!("{query} {word}") })).take(MOST).collect();
        }
        queries
    }

    /// Whether anything narrows the search besides its words.
    pub fn narrows(&self) -> bool {
        !(self.phrases.is_empty() && self.without.is_empty() && self.places.is_empty())
    }

    /// The tags asked for, each once.
    pub fn tags(&self) -> Vec<&str> {
        let mut tags: Vec<&str> = self.places.iter().flatten().filter_map(|place| if let Place::Tag(tag) = place { Some(tag.as_str()) } else { None }).collect();
        tags.sort_unstable();
        tags.dedup();
        tags
    }

    /// Whether the document at `path`, in the space named `space` (none: in no space), is in
    /// one place of each list asked for; `tagged` says whether it has a tag (or one under it).
    /// Folders and spaces are compared in any case.
    pub fn in_places(&self, path: &str, space: Option<&str>, tagged: impl Fn(&str) -> bool) -> bool {
        let folder = path.rsplit_once('/').map_or(String::new(), |(folder, _)| format!("/{}/", folder.to_lowercase()));
        self.places.iter().all(|any| {
            any.iter().any(|place| match place {
                Place::Tag(tag) => tagged(tag),
                Place::Folder(asked) => folder.contains(&format!("/{}/", asked.to_lowercase())),
                Place::Space(asked) => space.is_some_and(|name| name.to_lowercase() == asked.to_lowercase()),
            })
        })
    }

    /// Whether `name` and `text` hold every phrase and none of the words left out; with
    /// `ignore_case`, whatever their case.
    pub fn holds(&self, name: &str, text: &str, ignore_case: bool) -> bool {
        let fold = |s: &str| if ignore_case { s.to_lowercase() } else { s.to_owned() };
        let (name, text) = (fold(name), fold(text));
        self.phrases.iter().all(|phrase| text.contains(&fold(phrase)) || name.contains(&fold(phrase))) && !self.without.iter().any(|word| text.contains(&fold(word)) || name.contains(&fold(word)))
    }
}

/// The place a piece of the query names (`tag:work`, `тека:"Мої проєкти"`), if it names one.
fn place(token: &str) -> Option<Place> {
    let lower = token.to_lowercase();
    let value = |keys: &[&str]| keys.iter().find_map(|key| lower.strip_prefix(key).map(|_| token[key.len()..].trim_matches('"').to_owned())).filter(|value| !value.is_empty());
    if let Some(tag) = value(&["tag:", "тег:"]).map(|tag| tag.trim_start_matches('#').to_lowercase()).filter(|tag| !tag.is_empty()) {
        Some(Place::Tag(tag))
    } else if let Some(folder) = value(&["folder:", "тека:"]).map(|folder| folder.trim_matches('/').to_owned()).filter(|folder| !folder.is_empty()) {
        Some(Place::Folder(folder))
    } else {
        value(&["space:", "простір:"]).map(Place::Space)
    }
}

/// The query's pieces, spaces between them, what stands in quotes kept whole: each with
/// whether it was a quoted phrase of its own (not a key's quoted value).
fn tokens(query: &str) -> Vec<(String, bool)> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let (mut in_quotes, mut quoted, mut has_quote) = (false, false, false);
    for c in query.chars() {
        match c {
            '"' => {
                // Quotes that open a piece make it a phrase; after a key (`folder:"…"`) they
                // only keep its spaces.
                if !in_quotes && token.is_empty() {
                    quoted = true;
                } else if !in_quotes {
                    token.push('"');
                } else if !quoted {
                    token.push('"');
                }
                has_quote = true;
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !token.is_empty() {
                    tokens.push((std::mem::take(&mut token), quoted));
                }
                (quoted, has_quote) = (false, false);
            }
            c => token.push(c),
        }
    }
    let _ = has_quote;
    if !token.is_empty() {
        tokens.push((token, quoted));
    }
    tokens
}
