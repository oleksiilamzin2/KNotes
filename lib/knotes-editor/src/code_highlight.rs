//! Syntax highlighting of fenced code blocks (ADR 0025): syntect with the grammars bat ships
//! (`two-face`), each scope sorted into one of a few kinds the view colours, so colours follow
//! the appearance without highlighting again.

use std::ops::Range;
use std::sync::OnceLock;

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// What a stretch of code is, for its colour; plain code has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeToken {
    Comment,
    String,
    Number,
    Keyword,
    Type,
    Function,
    Constant,
    Preprocessor,
    Attribute,
}

static GRAMMARS: OnceLock<SyntaxSet> = OnceLock::new();

/// Reads the grammars (tens of milliseconds): called on another thread at launch, so the first
/// code block need not wait.
pub fn load_code_grammars() {
    grammars();
}

fn grammars() -> &'static SyntaxSet {
    GRAMMARS.get_or_init(two_face::syntax::extra_newlines)
}

/// The tokens of `code` in `language` (a fence's info: a language's name or extension, words
/// after it ignored), as byte ranges in order; `None` when the language is unknown or missing.
pub fn highlight_code(language: &str, code: &str) -> Option<Vec<(Range<usize>, CodeToken)>> {
    Some(CodeHighlighter::new(language)?.update(code).0)
}

/// A code block's highlighting kept between edits: each line's state at its start and its
/// tokens, so an edit parses again only the lines it changes (until the state after them is
/// what it was).
pub struct CodeHighlighter {
    /// The fence's info it was made for.
    language: String,
    syntax: &'static SyntaxReference,
    lines: Vec<Line>,
}

struct Line {
    text: String,
    /// The parser's state and scopes at the line's start, and at its end.
    start: (ParseState, ScopeStack),
    end: (ParseState, ScopeStack),
    /// Its tokens, from the line's start.
    tokens: Vec<(Range<usize>, CodeToken)>,
}

impl CodeHighlighter {
    /// For code in `language`; `None` when the language is unknown or missing.
    pub fn new(language: &str) -> Option<Self> {
        Some(Self { language: language.to_owned(), syntax: find_syntax(grammars(), language)?, lines: Vec::new() })
    }

    /// The fence's info it was made for.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// The tokens of `code` (byte ranges, in order) and how many lines were parsed for them.
    pub fn update(&mut self, code: &str) -> (Vec<(Range<usize>, CodeToken)>, usize) {
        let set = grammars();
        let mut state = (ParseState::new(self.syntax), ScopeStack::new());
        let mut lines = Vec::new();
        let mut parsed = 0;
        let mut old = std::mem::take(&mut self.lines).into_iter();
        for text in code.split_inclusive('\n') {
            // An unchanged line from the same state: its tokens and the state after it kept.
            match old.next().filter(|line| line.text == text && line.start == state) {
                Some(line) => {
                    state = line.end.clone();
                    lines.push(line);
                }
                None => {
                    parsed += 1;
                    let start = state.clone();
                    let tokens = parse_line(set, text, &mut state.0, &mut state.1);
                    lines.push(Line { text: text.to_owned(), start, end: state.clone(), tokens });
                }
            }
        }
        self.lines = lines;
        let mut tokens = Vec::new();
        let mut offset = 0;
        for line in &self.lines {
            tokens.extend(line.tokens.iter().map(|(r, k)| (offset + r.start..offset + r.end, *k)));
            offset += line.text.len();
        }
        (merge(tokens), parsed)
    }
}

/// Parses one line from `parser` and `stack`, leaving them as the line ends; gives its tokens.
fn parse_line(set: &SyntaxSet, text: &str, parser: &mut ParseState, stack: &mut ScopeStack) -> Vec<(Range<usize>, CodeToken)> {
    let mut tokens = Vec::new();
    let Ok(ops) = parser.parse_line(text, set) else { return tokens };
    let mut at = 0;
    for (pos, op) in ops {
        if let Some(kind) = token_of(stack).filter(|_| pos > at) {
            tokens.push((at..pos, kind));
        }
        let _ = stack.apply(&op);
        at = pos;
    }
    if let Some(kind) = token_of(stack).filter(|_| text.len() > at) {
        tokens.push((at..text.len(), kind));
    }
    tokens
}

/// Joins tokens of one kind that touch.
fn merge(tokens: Vec<(Range<usize>, CodeToken)>) -> Vec<(Range<usize>, CodeToken)> {
    let mut merged: Vec<(Range<usize>, CodeToken)> = Vec::with_capacity(tokens.len());
    for (range, kind) in tokens {
        match merged.last_mut() {
            Some((last, k)) if *k == kind && last.end == range.start => last.end = range.end,
            _ => merged.push((range, kind)),
        }
    }
    merged
}

/// The grammar for a fence's info: its first word, without the braces and dot some write.
fn find_syntax<'a>(set: &'a SyntaxSet, info: &str) -> Option<&'a SyntaxReference> {
    let word = info.trim().trim_start_matches(['{', '.']).split(|c: char| c.is_whitespace() || c == ',' || c == '}').next()?;
    if word.is_empty() {
        return None;
    }
    set.find_syntax_by_token(word).or_else(|| set.find_syntax_by_token(&word.to_lowercase()))
}

/// The kind of the innermost scope that says one (or says plain).
fn token_of(stack: &ScopeStack) -> Option<CodeToken> {
    let kinds = kinds();
    stack.as_slice().iter().rev().find_map(|scope| kinds.iter().find(|(prefix, _)| prefix.is_prefix_of(*scope)).map(|&(_, kind)| kind)).flatten()
}

/// Scope prefixes and their kinds, the first match winning; `None` is plain (operators stay
/// plain, as in Xcode, though their scopes start with `keyword`).
fn kinds() -> &'static [(Scope, Option<CodeToken>)] {
    static KINDS: OnceLock<Vec<(Scope, Option<CodeToken>)>> = OnceLock::new();
    KINDS.get_or_init(|| {
        use CodeToken::*;
        [
            ("comment", Some(Comment)),
            ("punctuation.definition.comment", Some(Comment)),
            ("string", Some(String)),
            ("constant.numeric", Some(Number)),
            ("constant.character.escape", Some(String)),
            ("constant", Some(Constant)),
            ("keyword.operator", None),
            ("keyword", Some(Keyword)),
            ("storage", Some(Keyword)),
            ("meta.preprocessor", Some(Preprocessor)),
            ("entity.name.function", Some(Function)),
            ("support.function", Some(Function)),
            ("variable.function", Some(Function)),
            ("entity.name.type", Some(Type)),
            ("entity.name.class", Some(Type)),
            ("entity.name.struct", Some(Type)),
            ("entity.name.enum", Some(Type)),
            ("entity.name.trait", Some(Type)),
            ("entity.other.inherited-class", Some(Type)),
            ("support.type", Some(Type)),
            ("support.class", Some(Type)),
            ("entity.name.tag", Some(Keyword)),
            ("entity.other.attribute-name", Some(Attribute)),
            ("meta.attribute", Some(Attribute)),
            ("meta.annotation", Some(Attribute)),
        ]
        .into_iter()
        .filter_map(|(prefix, kind)| Some((Scope::new(prefix).ok()?, kind)))
        .collect()
    })
}
