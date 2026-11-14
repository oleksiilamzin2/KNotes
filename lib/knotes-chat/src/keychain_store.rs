use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

use crate::{CredentialStore, Credentials};

const SERVICE: &str = "app.knotes.Knotes.chatgpt";
const ACCOUNT: &str = "tokens";

/// The tokens as one generic password in the login Keychain.
#[derive(Default)]
pub struct KeychainStore;

impl CredentialStore for KeychainStore {
    fn load(&self) -> Option<Credentials> {
        let bytes = get_generic_password(SERVICE, ACCOUNT).ok()?;
        Credentials::from_json(std::str::from_utf8(&bytes).ok()?)
    }

    fn save(&self, credentials: &Credentials) -> Result<(), String> {
        set_generic_password(SERVICE, ACCOUNT, credentials.to_json().as_bytes()).map_err(|e| e.to_string())
    }

    fn delete(&self) {
        let _ = delete_generic_password(SERVICE, ACCOUNT);
    }
}
