/// A space's colour: one of macOS's system colours, which suit light and dark appearance alike.
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
}

impl SpaceColor {
    pub const ALL: [Self; 9] = [Self::Blue, Self::Purple, Self::Pink, Self::Red, Self::Orange, Self::Yellow, Self::Green, Self::Teal, Self::Gray];

    /// Its name in the rules file.
    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Pink => "pink",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Gray => "gray",
        }
    }

    /// The colour a name gives; gray for a name it does not know.
    pub fn from_name(name: &str) -> Self {
        Self::ALL.into_iter().find(|c| c.name() == name).unwrap_or(Self::Gray)
    }
}
