//! The chat with an OpenAI model on the owner's ChatGPT plan (Sign in with ChatGPT), its
//! model reading the vault through the read-only tools (ADR 0027).

mod access_token;
mod authorized;
mod chat;
mod chat_error;
mod models;
mod sse;
mod tool_box;
mod transport;
mod ureq_transport;

pub use access_token::AccessToken;
pub use chat::{Chat, ChatEvent, INSTRUCTIONS};
pub use chat_error::ChatError;
pub use models::{Model, list_models};
pub use sse::{SseEvent, SseEvents};
pub use tool_box::ToolBox;
pub use transport::{HttpRequest, HttpResponse, Method, Transport};
pub use ureq_transport::UreqTransport;

/// Where the plan's requests go.
pub const API: &str = "https://api.openai.com/v1";
