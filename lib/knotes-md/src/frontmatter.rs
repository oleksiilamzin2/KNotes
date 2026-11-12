//! Obsidian properties from YAML frontmatter: `tags` and `aliases`.

use knotes_core::{Frontmatter, Span};
use yaml_rust2::{Yaml, YamlLoader};

pub(crate) fn parse_frontmatter(yaml: &str, span: Span) -> Frontmatter {
    let mut fm = Frontmatter { span, ..Frontmatter::default() };
    let Ok(docs) = YamlLoader::load_from_str(yaml) else { return fm };
    let Some(doc) = docs.first() else { return fm };

    for key in ["tags", "tag"] {
        collect(&doc[key], |c| c == ',' || c.is_whitespace(), &mut fm.tags);
    }
    fm.tags.retain_mut(|t| {
        if let Some(stripped) = t.strip_prefix('#') {
            *t = stripped.to_owned();
        }
        !t.is_empty() && !t.bytes().all(|b| b.is_ascii_digit())
    });

    for key in ["aliases", "alias"] {
        collect(&doc[key], |c| c == ',', &mut fm.aliases);
    }
    fm
}

/// A list gives one value per item; a plain string is split with `sep` (legacy Obsidian syntax).
fn collect(value: &Yaml, sep: fn(char) -> bool, out: &mut Vec<String>) {
    match value {
        Yaml::Array(items) => {
            for item in items {
                if let Some(s) = scalar(item) {
                    push_trimmed(&s, out);
                }
            }
        }
        Yaml::String(s) => s.split(sep).for_each(|part| push_trimmed(part, out)),
        _ => {}
    }
}

fn scalar(item: &Yaml) -> Option<String> {
    match item {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        _ => None,
    }
}

fn push_trimmed(s: &str, out: &mut Vec<String>) {
    let s = s.trim();
    if !s.is_empty() {
        out.push(s.to_owned());
    }
}
