//! Search by meaning's model, as the user chose it in the settings (ADR 0029), and its helper,
//! one for the whole app: the chat's search, the list's search and the thread keeping the
//! vectors share it. Choosing another model replaces the helper; those using it take the new
//! one when told (`VaultWindow::meaning_changed`).

use std::sync::{Arc, Mutex};

use knotes_embed::{EmbedError, Embedder, Passage, ProcessEmbedder, Vector};
use knotes_session::{AppSettings, app_data_dir};
use knotes_vectors::MeaningModel;

struct Chosen {
    model: Option<MeaningModel>,
    embedder: Option<Arc<Watched>>,
}

/// The helper, each request's outcome noted for the settings: one that does not start, a model
/// that will not load, fail silently otherwise (search by meaning becomes search by words, and
/// no vectors are made), the settings saying the vectors are up to date.
pub struct Watched(ProcessEmbedder);

impl Watched {
    /// Starts the helper ahead of a request (`ProcessEmbedder::warm`).
    pub fn warm(&self) {
        self.0.warm();
    }
}

impl Embedder for Watched {
    fn embed(&self, passages: &[Passage]) -> Result<Vec<Vector>, EmbedError> {
        let result = self.0.embed(passages);
        note_outcome(&result);
        result
    }
}

/// Why search by meaning does not work: its last request's failure; none once one works, or
/// another model is chosen.
static FAILING: Mutex<Option<String>> = Mutex::new(None);

fn note_outcome<T>(result: &Result<T, EmbedError>) {
    *FAILING.lock().unwrap_or_else(|e| e.into_inner()) = match result {
        Ok(_) => None,
        Err(EmbedError::Unavailable(why) | EmbedError::Failed(why)) => Some(why.clone()),
    };
}

/// Why search by meaning fails now, if it does.
pub fn failing() -> Option<String> {
    FAILING.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Probe: search by meaning failing for `why` (`KNOTES_PROBE_MEANING_FAILING`), the probe
/// starting no helper.
pub fn probe_fail(why: &str) {
    note_outcome::<()>(&Err(EmbedError::Failed(why.to_owned())));
}

/// Read from the settings at the first look.
static CHOSEN: Mutex<Option<Chosen>> = Mutex::new(None);

/// Grows with each model chosen and each store cleared: those holding a store open take it
/// as a sign to open it again.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The stores as they are now (see `GENERATION`).
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// The model chosen (the default until the user chooses); none when search by meaning is off.
pub fn model() -> Option<MeaningModel> {
    with(|chosen| chosen.model)
}

/// Its helper; none when off, in the probe (its windows never start a model) or without the
/// helper and the model's file beside the app.
pub fn embedder() -> Option<Arc<Watched>> {
    with(|chosen| chosen.embedder.clone())
}

/// The user chose `model` (none: off): kept in the settings, its helper made ready (it starts
/// at the first request).
pub fn choose(model: Option<MeaningModel>) {
    let app_data = app_data_dir();
    let mut settings = AppSettings::load(&app_data);
    settings.meaning_model = Some(MeaningModel::setting(model).to_string());
    if let Err(e) = settings.save(&app_data) {
        eprintln!("knotes: cannot save settings: {e}");
    }
    *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(chosen(model));
    *FAILING.lock().unwrap_or_else(|e| e.into_inner()) = None;
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn chosen(model: Option<MeaningModel>) -> Chosen {
    let embedder = if crate::probe::enabled() {
        None
    } else {
        model
            .and_then(|m| std::env::current_exe().ok().and_then(|exe| knotes_session::embedder_files(&exe, m.file())))
            .map(|(helper, file)| Arc::new(Watched(ProcessEmbedder::new(helper, file))))
    };
    Chosen { model, embedder }
}

fn with<T>(look: impl FnOnce(&Chosen) -> T) -> T {
    let mut held = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    let chosen = held.get_or_insert_with(|| chosen(MeaningModel::from_setting(AppSettings::load(&app_data_dir()).meaning_model.as_deref())));
    look(chosen)
}

/// Each vault's vectors being made: how far the pass has gone, and when it began.
static MAKING: Mutex<Vec<(std::path::PathBuf, knotes_vectors::Progress, std::time::Instant)>> = Mutex::new(Vec::new());

/// What the settings show of the vectors being made, in all vaults together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Making {
    pub done: usize,
    pub total: usize,
    /// Blocks a second, since the passes began.
    pub per_second: f64,
}

/// The thread keeping the vectors of the vault indexed in `dir` has gone as far as `progress`.
pub fn report(dir: &std::path::Path, progress: knotes_vectors::Progress) {
    let mut making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    match making.iter_mut().find(|(d, ..)| d == dir) {
        Some((_, kept, _)) => *kept = progress,
        None => making.push((dir.to_path_buf(), progress, std::time::Instant::now())),
    }
}

/// That thread's pass is over (done, stopped or failed).
pub fn finished(dir: &std::path::Path) {
    MAKING.lock().unwrap_or_else(|e| e.into_inner()).retain(|(d, ..)| d != dir);
}

/// The vectors being made now, if any.
pub fn making() -> Option<Making> {
    let making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    summary(&making.iter().map(|(_, p, began)| (*p, began.elapsed())).collect::<Vec<_>>())
}

fn summary(passes: &[(knotes_vectors::Progress, std::time::Duration)]) -> Option<Making> {
    if passes.is_empty() {
        return None;
    }
    let seconds = passes.iter().map(|(_, elapsed)| elapsed.as_secs_f64()).fold(0.0, f64::max).max(0.001);
    Some(Making {
        done: passes.iter().map(|(p, _)| p.done).sum(),
        total: passes.iter().map(|(p, _)| p.total).sum(),
        per_second: passes.iter().map(|(p, _)| p.passages).sum::<usize>() as f64 / seconds,
    })
}

/// Where the vaults' indexes are (`app_data/vaults/<hash>/`), each with its stores of vectors.
fn vaults_dir() -> std::path::PathBuf {
    app_data_dir().join("vaults")
}

/// The files of `model`'s vectors in every vault's folder under `vaults`: its store with
/// SQLite's journal beside it.
pub fn store_files_in(vaults: &std::path::Path, model: MeaningModel) -> Vec<std::path::PathBuf> {
    let store = model.store_file();
    let Ok(folders) = std::fs::read_dir(vaults) else { return Vec::new() };
    folders
        .flatten()
        .flat_map(|folder| ["", "-wal", "-shm"].map(|end| folder.path().join(format!("{store}{end}"))))
        .filter(|file| file.is_file())
        .collect()
}

/// How many bytes `model`'s vectors take in all vaults.
pub fn cache_size(model: MeaningModel) -> u64 {
    store_files_in(&vaults_dir(), model).iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum()
}

/// `model`'s vectors removed from every vault: embedded again when the model is next used. The
/// caller tells the windows when `model` is the one chosen, so they open fresh stores.
pub fn clear_cache(model: MeaningModel) {
    for file in store_files_in(&vaults_dir(), model) {
        if let Err(e) = std::fs::remove_file(&file) {
            eprintln!("knotes: cannot remove {}: {e}", file.display());
        }
    }
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use knotes_vectors::{MeaningModel, Progress};

    #[test]
    fn a_failing_request_is_kept_for_the_settings_until_one_works() {
        super::note_outcome::<()>(&Err(knotes_embed::EmbedError::Unavailable("no model".into())));
        assert_eq!(super::failing().as_deref(), Some("no model"));
        super::note_outcome(&Ok(()));
        assert_eq!(super::failing(), None);
    }

    #[test]
    fn the_vaults_passes_add_up_and_their_speed_is_blocks_a_second() {
        assert_eq!(super::summary(&[]), None, "nothing being made");
        let passes = [(Progress { done: 10, total: 40, passages: 300 }, Duration::from_secs(2)), (Progress { done: 5, total: 10, passages: 100 }, Duration::from_secs(1))];
        assert_eq!(super::summary(&passes), Some(super::Making { done: 15, total: 50, per_second: 200.0 }));
    }

    #[test]
    fn a_model_s_vectors_are_found_in_every_vault_with_their_journal_and_no_other_s() {
        let vaults = tempfile::tempdir().unwrap();
        for (folder, file, bytes) in [("a", "vectors-e5-large-instruct.sqlite", 100), ("a", "vectors-e5-large-instruct.sqlite-wal", 20), ("b", "vectors-e5-large-instruct.sqlite", 50), ("b", "vectors-embeddinggemma-2.sqlite", 7), ("b", "index.sqlite", 999)] {
            std::fs::create_dir_all(vaults.path().join(folder)).unwrap();
            std::fs::write(vaults.path().join(folder).join(file), vec![0u8; bytes]).unwrap();
        }
        let size = |model| super::store_files_in(vaults.path(), model).iter().map(|f| f.metadata().unwrap().len()).sum::<u64>();
        assert_eq!((size(MeaningModel::E5Large), size(MeaningModel::EmbeddingGemma2)), (170, 7));
        assert!(super::store_files_in(&vaults.path().join("none"), MeaningModel::E5Large).is_empty());
    }
}
