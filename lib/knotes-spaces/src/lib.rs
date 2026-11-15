//! Spaces (ADR 0030): named, coloured sets of a vault's folders, read from its rules files.

mod space;
mod space_color;
mod spaces;

pub use space::Space;
pub use space_color::SpaceColor;
pub use spaces::Spaces;
