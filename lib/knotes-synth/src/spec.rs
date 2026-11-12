/// Shape of a synthetic vault.
#[derive(Debug, Clone)]
pub struct VaultSpec {
    pub notes: usize,
    pub seed: u64,
    pub folders: usize,
    /// Note sizes are log-normal: median and sigma of `ln(bytes)`, capped at `max_bytes`.
    pub median_bytes: f64,
    pub sigma: f64,
    pub max_bytes: usize,
    /// Share of notes written in Ukrainian; the rest are English.
    pub ukrainian_share: f64,
    /// Share of wikilinks that point to a note that does not exist.
    pub broken_link_share: f64,
}

impl VaultSpec {
    pub fn new(notes: usize, seed: u64) -> Self {
        Self {
            notes,
            seed,
            folders: (notes / 500).clamp(4, 200),
            median_bytes: 2_500.0,
            sigma: 1.2,
            max_bytes: 300_000,
            ukrainian_share: 0.6,
            broken_link_share: 0.04,
        }
    }
}
