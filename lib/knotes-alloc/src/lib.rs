//! Global allocator of Knotes (ADR 0014).

mod direct_large;

pub use direct_large::{DirectLarge, LARGE};
