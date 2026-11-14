use std::sync::{Mutex, MutexGuard};

use serde_json::Value;

use crate::token_endpoint::{DISCOVERY, RESOURCE, TokenFailure, now, post_form, post_token, read_json};
use crate::{AccessToken, ChatError, CredentialStore, Credentials, HttpRequest, Method, Transport};

/// A token this close to its end is refreshed before use (seconds).
const REFRESH_AHEAD: u64 = 5 * 60;
/// Refresh errors that mean the refresh token is no good.
const UNUSABLE: [&str; 6] = ["invalid_grant", "invalid_refresh_token", "token_expired", "refresh_token_expired", "refresh_token_invalidated", "refresh_token_reused"];

/// The signed-in ChatGPT account: its tokens, refreshed as they run out (one refresh at a
/// time: the refresh token changes with each), kept in `store`.
pub struct ChatGptAccount<T, S> {
    transport: T,
    store: S,
    credentials: Mutex<Option<Credentials>>,
}

impl<T: Transport, S: CredentialStore> ChatGptAccount<T, S> {
    /// The account the store keeps, if any.
    pub fn new(transport: T, store: S) -> Self {
        let credentials = Mutex::new(store.load());
        Self { transport, store, credentials }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn is_signed_in(&self) -> bool {
        self.lock().is_some()
    }

    /// The signed-in account's email.
    pub fn email(&self) -> Option<String> {
        self.lock().as_ref().and_then(|c| c.email.clone())
    }

    /// The last ID token, to name the account when signing in again.
    pub fn id_token_hint(&self) -> Option<String> {
        self.lock().as_ref().map(|c| c.id_token.clone()).filter(|t| !t.is_empty())
    }

    /// Makes a completed sign-in the account.
    pub fn accept(&self, credentials: Credentials) -> Result<(), String> {
        self.store.save(&credentials)?;
        *self.lock() = Some(credentials);
        Ok(())
    }

    /// Ends the renewable session at OpenAI (as best it can) and forgets the tokens.
    pub fn sign_out(&self) {
        let Some(credentials) = self.lock().take() else { return };
        self.store.delete();
        let discovery = HttpRequest { method: Method::Get, url: DISCOVERY.into(), headers: vec![("Accept".into(), "application/json".into())], body: Vec::new() };
        let Ok(mut response) = self.transport.send(&discovery) else { return };
        if let Some(endpoint) = read_json(&mut response).get("revocation_endpoint").and_then(Value::as_str) {
            let fields = [("token", credentials.refresh_token.as_str()), ("token_type_hint", "refresh_token"), ("client_id", credentials.client_id.as_str())];
            let _ = post_form(&self.transport, endpoint, &fields);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Credentials>> {
        self.credentials.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Refreshes the tokens in `slot`; an unusable refresh token signs out.
    fn refresh(&self, slot: &mut Option<Credentials>) -> Result<String, ChatError> {
        let Some(old) = slot.as_ref() else { return Err(ChatError::SignInAgain("not signed in".into())) };
        let fields = [("grant_type", "refresh_token"), ("client_id", old.client_id.as_str()), ("refresh_token", old.refresh_token.as_str()), ("resource", RESOURCE)];
        match post_token(&self.transport, &fields) {
            Ok(reply) => {
                let mut new = old.clone();
                new.access_token = reply.access_token;
                new.refresh_token = reply.refresh_token.unwrap_or(new.refresh_token);
                new.id_token = reply.id_token.unwrap_or(new.id_token);
                new.scopes = reply.scopes.unwrap_or(new.scopes);
                new.expires_at = now() + reply.expires_in;
                let token = new.access_token.clone();
                let _ = self.store.save(&new);
                *slot = Some(new);
                Ok(token)
            }
            Err(TokenFailure::Refused(code, words)) if UNUSABLE.contains(&code.as_str()) => {
                *slot = None;
                self.store.delete();
                Err(ChatError::SignInAgain(if words.is_empty() { code } else { words }))
            }
            Err(TokenFailure::Refused(code, words)) => Err(ChatError::Failed(format!("refresh: {code}: {words}"))),
            Err(TokenFailure::Unavailable(words)) => Err(ChatError::Unavailable(words)),
            Err(TokenFailure::Network(words)) => Err(ChatError::Network(words)),
        }
    }
}

impl<T: Transport, S: CredentialStore> AccessToken for ChatGptAccount<T, S> {
    fn token(&self) -> Result<String, ChatError> {
        let mut slot = self.lock();
        match slot.as_ref() {
            None => Err(ChatError::SignInAgain("not signed in".into())),
            Some(c) if c.expires_at > now() + REFRESH_AHEAD => Ok(c.access_token.clone()),
            Some(_) => self.refresh(&mut slot),
        }
    }

    fn renew(&self) -> Result<String, ChatError> {
        let mut slot = self.lock();
        self.refresh(&mut slot)
    }
}
