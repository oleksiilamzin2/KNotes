use std::path::{Path, PathBuf};

/// `app_data/vaults/<hash of the canonical vault path>`: the index lives outside the vault
/// (ADR 0003), one folder per vault.
pub fn index_dir(app_data: &Path, vault: &Path) -> PathBuf {
    let canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    app_data.join("vaults").join(format!("{:016x}", fnv1a(canonical.as_os_str().as_encoded_bytes())))
}

/// FNV-1a, 64 bit: stable across runs and Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}
