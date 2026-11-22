//! What the right panel shows (ADR 0023).

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PanelMode {
    /// The AI (the chat).
    #[default]
    Ai,
    /// The outline of the document open.
    Outline,
    /// The connections of the document open (ADR 0026).
    Links,
    /// The starred folders and files.
    Stars,
}

impl PanelMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Outline => "outline",
            Self::Links => "links",
            Self::Stars => "stars",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "ai" => Some(Self::Ai),
            "outline" => Some(Self::Outline),
            "links" => Some(Self::Links),
            "stars" => Some(Self::Stars),
            _ => None,
        }
    }
}
