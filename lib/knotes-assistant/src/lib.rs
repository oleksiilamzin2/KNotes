//! What the app's assistant does off the main thread, with no window: the chat's turns over
//! the vault's tools (ADR 0027), the thread keeping the vault's vectors up to its index (ADR
//! 0028), and the model chosen for search by meaning with its helper (ADR 0029).

pub mod chat_eval;
pub mod chat_worker;
pub mod meaning;
