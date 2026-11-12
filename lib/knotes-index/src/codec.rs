//! Stable integer codes of enums stored in the index.

use knotes_core::{FileKind, LinkKind};

pub(crate) fn file_kind_code(kind: FileKind) -> i64 {
    match kind {
        FileKind::Markdown => 0,
        FileKind::Canvas => 1,
        FileKind::Pdf => 2,
        FileKind::Image => 3,
        FileKind::Other => 4,
    }
}

pub(crate) fn link_kind_code(kind: LinkKind) -> i64 {
    match kind {
        LinkKind::Wiki => 0,
        LinkKind::Markdown => 1,
        LinkKind::External => 2,
    }
}

pub(crate) fn link_kind_from(code: i64) -> LinkKind {
    match code {
        0 => LinkKind::Wiki,
        1 => LinkKind::Markdown,
        _ => LinkKind::External,
    }
}
