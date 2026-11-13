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
    let set = grammars();
    let syntax = find_syntax(set, language)?;
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut tokens: Vec<(Range<usize>, CodeToken)> = Vec::new();
    let mut push = |range: Range<usize>, kind: Option<CodeToken>| {
        let Some(kind) = kind.filter(|_| !range.is_empty()) else { return };
        match tokens.last_mut() {
            Some((last, k)) if *k == kind && last.end == range.start => last.end = range.end,
            _ => tokens.push((range, kind)),
        }
    };
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        let ops = state.parse_line(line, set).ok()?;
        let mut at = 0;
        for (pos, op) in ops {
            push(offset + at..offset + pos, token_of(&stack));
            stack.apply(&op).ok()?;
            at = pos;
        }
        push(offset + at..offset + line.len(), token_of(&stack));
        offset += line.len();
    }
    Some(tokens)
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
