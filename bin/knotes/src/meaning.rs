//! Search by meaning's model, as the user chose it in the settings (ADR 0029), and its helper,
//! one for the whole app: the chat's search, the list's search and the thread keeping the
//! vectors share it. Choosing another model replaces the helper; those using it take the new
//! one when told (`VaultWindow::meaning_changed`).

use std::sync::{Arc, Mutex};

use knotes_embed::ProcessEmbedder;
use knotes_session::{AppSettings, app_data_dir};
use knotes_vectors::MeaningModel;

struct Chosen {
    model: Option<MeaningModel>,
    embedder: Option<Arc<ProcessEmbedder>>,
}

/// Read from the settings at the first look.
static CHOSEN: Mutex<Option<Chosen>> = Mutex::new(None);

/// The model chosen (the default until the user chooses); none when search by meaning is off.
pub fn model() -> Option<MeaningModel> {
    with(|chosen| chosen.model)
}

/// Its helper; none when off, in the probe (its windows never start a model) or without the
/// helper and the model's file beside the app.
pub fn embedder() -> Option<Arc<ProcessEmbedder>> {
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
}

fn chosen(model: Option<MeaningModel>) -> Chosen {
    let embedder = if crate::probe::enabled() {
        None
    } else {
        model
            .and_then(|m| std::env::current_exe().ok().and_then(|exe| knotes_session::embedder_files(&exe, m.file())))
            .map(|(helper, file)| Arc::new(ProcessEmbedder::new(helper, file)))
    };
    Chosen { model, embedder }
}

fn with<T>(look: impl FnOnce(&Chosen) -> T) -> T {
    let mut held = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    let chosen = held.get_or_insert_with(|| chosen(MeaningModel::from_setting(AppSettings::load(&app_data_dir()).meaning_model.as_deref())));
    look(chosen)
}
