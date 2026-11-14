use crate::Credentials;

/// Where the tokens are kept between launches: the Keychain in the app, memory in tests.
pub trait CredentialStore {
    fn load(&self) -> Option<Credentials>;
    fn save(&self, credentials: &Credentials) -> Result<(), String>;
    fn delete(&self);
}

impl<C: CredentialStore + ?Sized> CredentialStore for Box<C> {
    fn load(&self) -> Option<Credentials> {
        (**self).load()
    }

    fn save(&self, credentials: &Credentials) -> Result<(), String> {
        (**self).save(credentials)
    }

    fn delete(&self) {
        (**self).delete()
    }
}
