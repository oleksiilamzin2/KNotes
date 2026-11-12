use std::collections::HashSet;
use std::path::Path;
use std::{fs, io};

use rand::{RngExt, SeedableRng};
use rayon::prelude::*;

use crate::note_writer::{LinkTargets, NoteWriter, TagSet};
use crate::vocabulary::Vocabulary;
use crate::word_source::WordSource;
use crate::zipf::Zipf;
use crate::{Rng, VaultSpec};

const UK_VOCABULARY: usize = 40_000;
const EN_VOCABULARY: usize = 30_000;
const TAGS: usize = 300;

/// The plan of a synthetic vault: folders, titles and link popularity. Note texts are
/// generated on demand from `(seed, index)`, so any note can be produced alone and in parallel.
pub struct SyntheticVault {
    spec: VaultSpec,
    uk: Vocabulary,
    en: Vocabulary,
    folders: Vec<String>,
    titles: Vec<String>,
    note_folder: Vec<u32>,
    note_is_uk: Vec<bool>,
    hubs: Vec<u32>,
    hub_zipf: Zipf,
    tags: Vec<String>,
    tag_zipf: Zipf,
}

impl SyntheticVault {
    pub fn plan(spec: VaultSpec) -> Self {
        let mut rng = Rng::seed_from_u64(spec.seed);
        let uk = Vocabulary::ukrainian(UK_VOCABULARY, &mut rng);
        let en = Vocabulary::english(EN_VOCABULARY, &mut rng);
        let pick = |is_uk: bool| -> &dyn WordSource { if is_uk { &uk } else { &en } };

        let mut folders: Vec<String> = Vec::with_capacity(spec.folders);
        let mut seen = HashSet::new();
        for i in 0..spec.folders.max(1) {
            let is_uk = rng.random_bool(spec.ukrainian_share);
            let name = capitalized(pick(is_uk).content_word(&mut rng));
            let parent = (i > 0 && rng.random_bool(0.5))
                .then(|| &folders[rng.random_range(0..folders.len())])
                .filter(|p| p.matches('/').count() < 2);
            let mut path = parent.map_or(name.clone(), |p| format!("{p}/{name}"));
            if !seen.insert(path.clone()) {
                path = format!("{path} {i}");
                seen.insert(path.clone());
            }
            folders.push(path);
        }

        let n = spec.notes;
        let note_is_uk: Vec<bool> = (0..n).map(|_| rng.random_bool(spec.ukrainian_share)).collect();
        let mut seen = HashSet::with_capacity(n);
        let titles: Vec<String> = (0..n)
            .map(|i| {
                let words = pick(note_is_uk[i]);
                let count = rng.random_range(1..=5);
                let mut title = capitalized(words.content_word(&mut rng));
                for _ in 1..count {
                    title.push(' ');
                    title.push_str(words.content_word(&mut rng));
                }
                if !seen.insert(title.clone()) {
                    title = format!("{title} {i}");
                    seen.insert(title.clone());
                }
                title
            })
            .collect();
        let note_folder = (0..n).map(|_| rng.random_range(0..folders.len()) as u32).collect();

        let mut hubs: Vec<u32> = (0..n as u32).collect();
        for i in (1..hubs.len()).rev() {
            hubs.swap(i, rng.random_range(0..=i));
        }
        let hub_zipf = Zipf::new(n, 0.8, 10.0);

        let mut tags: Vec<String> = Vec::with_capacity(TAGS);
        while tags.len() < TAGS {
            let is_uk = rng.random_bool(spec.ukrainian_share);
            let words = pick(is_uk);
            let mut tag = words.content_word(&mut rng).to_owned();
            if tags.len().is_multiple_of(3) {
                tag.push('/');
                tag.push_str(words.content_word(&mut rng));
            }
            if !tag.contains('\'') {
                tags.push(tag);
            }
        }
        let tag_zipf = Zipf::new(TAGS, 1.0, 1.0);

        Self { spec, uk, en, folders, titles, note_folder, note_is_uk, hubs, hub_zipf, tags, tag_zipf }
    }

    pub fn len(&self) -> usize {
        self.titles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }

    pub fn title(&self, i: usize) -> &str {
        &self.titles[i]
    }

    /// Vault-relative path, `/`-separated.
    pub fn path(&self, i: usize) -> String {
        format!("{}/{}.md", self.folders[self.note_folder[i] as usize], self.titles[i])
    }

    pub fn note(&self, i: usize) -> String {
        let mut rng = Rng::seed_from_u64(self.spec.seed ^ (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let target = self.target_size(&mut rng);
        let links = LinkTargets {
            titles: &self.titles,
            hubs: &self.hubs,
            zipf: &self.hub_zipf,
            broken_share: self.spec.broken_link_share,
        };
        let tags = TagSet { tags: &self.tags, zipf: &self.tag_zipf };
        let words = if self.note_is_uk[i] { &self.uk } else { &self.en };
        let writer = NoteWriter { out: String::with_capacity(target + 1024), rng, words, links: &links, tags: &tags };
        writer.write(&self.titles[i], target)
    }

    /// Writes all notes under `dir` in parallel; returns the total number of bytes written.
    pub fn write_to(&self, dir: &Path) -> io::Result<u64> {
        for folder in &self.folders {
            fs::create_dir_all(dir.join(folder))?;
        }
        let sizes: Vec<u64> = (0..self.len())
            .into_par_iter()
            .map(|i| {
                let text = self.note(i);
                fs::write(dir.join(self.path(i)), &text).map(|_| text.len() as u64)
            })
            .collect::<io::Result<_>>()?;
        Ok(sizes.iter().sum())
    }

    /// Log-normal size (Box–Muller), capped to `30..=max_bytes`.
    fn target_size(&self, rng: &mut Rng) -> usize {
        let u1 = 1.0 - rng.random::<f64>();
        let u2 = rng.random::<f64>();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        let bytes = (self.spec.median_bytes.ln() + self.spec.sigma * z).exp();
        (bytes as usize).clamp(30, self.spec.max_bytes)
    }
}

fn capitalized(w: &str) -> String {
    let mut chars = w.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().chain(chars).collect())
}
