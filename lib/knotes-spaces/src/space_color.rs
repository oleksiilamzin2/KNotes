/// A space's colour: one of macOS's system colours, which suit light and dark appearance alike,
/// or one of the user's own (red, green, blue), chosen in the colour panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpaceColor {
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Gray,
    Custom(u8, u8, u8),
}

impl SpaceColor {
    pub const ALL: [Self; 9] = [Self::Blue, Self::Purple, Self::Pink, Self::Red, Self::Orange, Self::Yellow, Self::Green, Self::Teal, Self::Gray];

    /// Its name in the rules file: a system colour's, or `#rrggbb` for one of one's own.
    pub fn name(self) -> String {
        let name = match self {
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Pink => "pink",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Gray => "gray",
            Self::Custom(r, g, b) => return format!("#{r:02x}{g:02x}{b:02x}"),
        };
        name.to_owned()
    }

    /// The colour a name gives (`#rrggbb` or `#rgb` for one's own); gray for a name it does
    /// not know.
    pub fn from_name(name: &str) -> Self {
        if let Some(hex) = name.strip_prefix('#') {
            let digits: Option<Vec<u8>> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect();
            return match digits.as_deref() {
                Some([r1, r2, g1, g2, b1, b2]) => Self::Custom(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2),
                Some([r, g, b]) => Self::Custom(r * 17, g * 17, b * 17),
                _ => Self::Gray,
            };
        }
        Self::ALL.into_iter().find(|c| c.name() == name).unwrap_or(Self::Gray)
    }
}
