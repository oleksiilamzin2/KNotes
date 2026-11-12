//! Deterministic synthetic Obsidian vaults for tests and benchmarks.

mod note_writer;
mod spec;
mod vault;
mod vocabulary;
mod word_source;
mod zipf;

pub use spec::VaultSpec;
pub use vault::SyntheticVault;

type Rng = rand_chacha::ChaCha8Rng;
