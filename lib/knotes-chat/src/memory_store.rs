use std::sync::Mutex;

use crate::{CredentialStore, Credentials};

/// Tokens kept only while the app runs (tests, the probe).
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<Credentials>>);

impl CredentialStore for MemoryStore {
    fn load(&self) -> Option<Credentials> {
        self.0.lock().unwrap().clone()
    }

    fn save(&self, credentials: &Credentials) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(credentials.clone());
        Ok(())
    }

    fn delete(&self) {
        *self.0.lock().unwrap() = None;
    }
}
