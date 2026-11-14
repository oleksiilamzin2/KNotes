use crate::Credentials;

/// Where the tokens are kept between launches: the Keychain in the app, memory in tests.
pub trait CredentialStore {
    fn load(&self) -> Option<Credentials>;
    fn save(&self, credentials: &Credentials) -> Result<(), String>;
    fn delete(&self);
}
