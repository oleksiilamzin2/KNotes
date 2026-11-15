use crate::SpaceColor;

/// A space as the vault's root lists it: its id (what folders name), its name and colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub color: SpaceColor,
}
