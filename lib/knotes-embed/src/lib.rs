//! Search by meaning (ADR 0028): what text the model is given (`Passage`), the vectors it gives
//! back cut and kept in bytes (`Vector`), the trait that makes them (`Embedder`), and the line
//! protocol of the helper process that runs the model.

mod embed_error;
mod embedder;
mod passage;
mod process_embedder;
pub mod protocol;
mod vector;

pub use embed_error::EmbedError;
pub use embedder::Embedder;
pub use passage::{Passage, PromptStyle};
pub use process_embedder::ProcessEmbedder;
pub use vector::{DIMS, Vector, quantize, similarity};
