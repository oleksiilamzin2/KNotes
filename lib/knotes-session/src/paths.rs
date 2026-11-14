use std::path::{Path, PathBuf};

/// `~/Library/Application Support/Knotes`: the app's settings and its vaults' indexes.
pub fn app_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/Knotes")
}

/// `app_data/vaults/<hash of the canonical vault path>`: the index lives outside the vault
/// (ADR 0003), one folder per vault.
pub fn index_dir(app_data: &Path, vault: &Path) -> PathBuf {
    let canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    app_data.join("vaults").join(format!("{:016x}", fnv1a(canonical.as_os_str().as_encoded_bytes())))
}

/// The embedding model's file (ADR 0028): in the bundle's Resources, or `models/` in the
/// repository while developing (`scripts/setup.sh` fetches it).
pub const EMBEDDING_MODEL_FILE: &str = "embeddinggemma-300M-Q8_0.gguf";

/// The helper that runs the embedding model and the model, for the program at `exe`: the helper
/// beside it (`Contents/MacOS/knotes-embed`, or `target/<profile>/knotes-embed` while
/// developing), the model in `Contents/Resources`, else in the repository's `models/`. None
/// when either is missing: search goes on by words alone.
pub fn embedder_files(exe: &Path) -> Option<(PathBuf, PathBuf)> {
    let dir = exe.parent()?;
    let helper = dir.join("knotes-embed");
    let bundled = dir.parent().map(|contents| contents.join("Resources").join(EMBEDDING_MODEL_FILE));
    let developing = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models").join(EMBEDDING_MODEL_FILE);
    let model = bundled.filter(|m| m.is_file()).or_else(|| developing.is_file().then_some(developing))?;
    helper.is_file().then_some((helper, model))
}

/// FNV-1a, 64 bit: stable across runs and Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}
