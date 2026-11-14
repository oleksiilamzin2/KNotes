//! EmbeddingGemma through llama.cpp on the GPU (Metal): the embedder the helper process runs
//! (ADR 0028). Kept out of the app and the MCP server, which talk to the helper instead.

mod llama_embedder;

pub use llama_embedder::LlamaEmbedder;
