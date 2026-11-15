use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;

use knotes_embed::{EmbedError, Embedder, Passage, PromptStyle, Vector, quantize};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;

/// The tokens one decode takes, all passages of a batch together; a passage longer is cut
/// (the model's own window is 2048).
const WINDOW: usize = 2048;
/// The passages one decode takes at most.
const SEQUENCES: usize = 32;

/// llama.cpp starts once a process.
static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();

/// The model loaded on the GPU. Each request runs in a context of its own, made for it: the
/// model is then freed whole when the embedder is dropped, as Metal checks at exit.
pub struct LlamaEmbedder {
    backend: &'static LlamaBackend,
    model: LlamaModel,
    /// Told by the model's name (or its file's): e5, or EmbeddingGemma.
    style: PromptStyle,
    /// The tokens a decode takes: `WINDOW`, or less for a model trained on less (e5, 512).
    window: usize,
}

fn backend() -> Result<&'static LlamaBackend, EmbedError> {
    BACKEND
        .get_or_init(|| {
            let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
            backend.void_logs();
            Ok(backend)
        })
        .as_ref()
        .map_err(|e| EmbedError::Unavailable(format!("llama.cpp: {e}")))
}

impl LlamaEmbedder {
    /// Loads the model at `path` with every layer on the GPU.
    pub fn load(path: &Path) -> Result<Self, EmbedError> {
        // llama-cpp-2 asserts the file is there (in debug builds) rather than saying it is not.
        if !path.is_file() {
            return Err(EmbedError::Unavailable(format!("no model at {}", path.display())));
        }
        let backend = backend()?;
        let model = LlamaModel::load_from_file(backend, path, &LlamaModelParams::default().with_n_gpu_layers(u32::MAX))
            .map_err(|e| EmbedError::Unavailable(format!("{}: {e}", path.display())))?;
        // The model's name, or its file's when the file names none.
        let name = model.meta_val_str("general.name").ok().filter(|n| !n.is_empty()).unwrap_or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()).to_lowercase();
        let style = if name.contains("e5") { PromptStyle::E5Instruct } else { PromptStyle::EmbeddingGemma };
        let window = WINDOW.min(model.n_ctx_train() as usize);
        Ok(Self { backend, model, style, window })
    }
}

impl Embedder for LlamaEmbedder {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
        let vocab = self.model.vocab();
        let tokens: Vec<_> = passages
            .iter()
            .map(|p| {
                let mut t = vocab.tokenize(p.prompt_in(self.style).as_bytes(), true, false);
                t.truncate(self.window);
                t
            })
            .collect();
        let params = LlamaContextParams::default()
            .with_embeddings(true)
            .with_n_ctx(NonZeroU32::new(self.window as u32))
            .with_n_batch(self.window as u32)
            .with_n_ubatch(self.window as u32)
            .with_n_seq_max(SEQUENCES as u32);
        let mut context = self.model.new_context(self.backend, params).map_err(|e| EmbedError::Failed(format!("context: {e}")))?;
        let mut vectors = Vec::with_capacity(passages.len());
        let mut batch = LlamaBatch::new(self.window, SEQUENCES as i32);
        let mut start = 0;
        while start < tokens.len() {
            // As many passages as fit in one decode.
            let (mut end, mut used) = (start, 0);
            while end < tokens.len() && end - start < SEQUENCES && (end == start || used + tokens[end].len() <= self.window) {
                used += tokens[end].len();
                end += 1;
            }
            batch.clear();
            for (seq, t) in tokens[start..end].iter().enumerate() {
                batch.add_sequence(t, seq as i32, true).map_err(|e| EmbedError::Failed(format!("batch: {e}")))?;
            }
            context.clear_kv_cache();
            context.decode(&mut batch).map_err(|e| EmbedError::Failed(format!("decode: {e}")))?;
            for seq in 0..end - start {
                let full = context.embeddings_seq_ith(seq as i32).map_err(|e| EmbedError::Failed(format!("embeddings: {e}")))?;
                vectors.push(quantize(full));
            }
            start = end;
        }
        Ok(vectors)
    }
}
