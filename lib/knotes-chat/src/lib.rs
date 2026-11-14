//! The chat with an OpenAI model on the owner's ChatGPT plan (Sign in with ChatGPT), its
//! model reading the vault through the read-only tools, and signing in for it (ADR 0027).

mod access_token;
mod authorized;
pub mod base64url;
mod chat;
mod chat_error;
mod chatgpt_account;
mod context;
mod conversation;
mod conversations;
mod credential_store;
mod credentials;
mod crypto;
mod form;
mod id_token;
mod keychain_store;
mod memory_store;
mod models;
mod registration;
mod reply_runs;
mod sign_in;
mod sse;
mod token_endpoint;
mod tool_box;
mod transport;
mod ureq_transport;

pub use access_token::AccessToken;
pub use chat::{Chat, ChatEvent, INSTRUCTIONS, SUMMARY_INSTRUCTIONS};
pub use chat_error::ChatError;
pub use chatgpt_account::ChatGptAccount;
pub use context::{KEEP_TURNS, Memory, PRICED_WINDOW, Summary, compact_at, context, estimate_tokens};
pub use conversation::{Conversation, Shown};
pub use conversations::{ConversationEntry, Conversations};
pub use credential_store::CredentialStore;
pub use credentials::Credentials;
pub use crypto::pkce_challenge;
pub use form::{form_encode, parse_query, percent_encode};
pub use id_token::{IdClaims, check_id_token};
pub use keychain_store::KeychainStore;
pub use memory_store::MemoryStore;
pub use models::{Model, list_models};
pub use registration::{REGISTRATION_FILE, Registration};
pub use reply_runs::{Run, RunLink, RunStyle, reply_runs};
pub use sign_in::{SignIn, SignInError};
pub use sse::{SseEvent, SseEvents};
pub use tool_box::ToolBox;
pub use transport::{HttpRequest, HttpResponse, Method, Transport};
pub use ureq_transport::UreqTransport;

/// Where the plan's requests go.
pub const API: &str = "https://api.openai.com/v1";
