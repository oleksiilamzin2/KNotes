use std::fmt;

use serde_json::Value;

/// What went wrong, sorted by what to do about it (OpenAI's errors-and-recovery guide).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatError {
    /// The plan's (or the app's) usage limit: pause and point at "Manage usage".
    UsageLimit,
    /// Usage or the user could not be checked just now: try again later.
    Unavailable(String),
    /// The token is no good and cannot be renewed: sign in again.
    SignInAgain(String),
    /// The plan cannot be used this way (user, workspace or policy).
    NotEligible(String),
    /// The request asked for something this route does not take (the parameter named).
    Unsupported(String),
    /// The reply stopped before its end (the reason given).
    Incomplete(String),
    /// The stream ended without saying the reply was complete.
    Interrupted,
    Cancelled,
    /// The model kept calling tools past the limit.
    TooManySteps,
    /// No response: no network, a timeout.
    Network(String),
    Failed(String),
}

impl ChatError {
    /// The error of an OpenAI error object (`{code, message, param}`) under HTTP `status`
    /// (0 inside a stream).
    pub fn from_openai(error: &Value, status: u16) -> Self {
        let text = |key: &str| error.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let (code, message) = (text("code"), text("message"));
        match code.as_str() {
            "subscription_sharing_usage_limit_exceeded" => Self::UsageLimit,
            "subscription_sharing_usage_unavailable" | "subscription_sharing_user_unavailable" => Self::Unavailable(message),
            "subscription_sharing_invalid_user" => Self::SignInAgain(message),
            "subscription_sharing_user_not_eligible" => Self::NotEligible(message),
            "subscription_sharing_unsupported_capability" => Self::Unsupported(Some(text("param")).filter(|p| !p.is_empty()).unwrap_or(message)),
            _ if status == 401 => Self::SignInAgain(message),
            _ if status >= 500 => Self::Unavailable(message),
            "" => Self::Failed(if message.is_empty() { format!("HTTP {status}") } else { message }),
            _ => Self::Failed(format!("{code}: {message}")),
        }
    }
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::UsageLimit => write!(f, "ChatGPT plan usage limit reached"),
            Self::Unavailable(m) => write!(f, "ChatGPT is unavailable for now: {m}"),
            Self::SignInAgain(m) => write!(f, "sign in again: {m}"),
            Self::NotEligible(m) => write!(f, "ChatGPT plan usage is not available: {m}"),
            Self::Unsupported(m) => write!(f, "not supported with a ChatGPT plan: {m}"),
            Self::Incomplete(m) => write!(f, "the reply stopped early: {m}"),
            Self::Interrupted => write!(f, "the reply was cut off"),
            Self::Cancelled => write!(f, "stopped"),
            Self::TooManySteps => write!(f, "too many tool calls"),
            Self::Network(m) => write!(f, "network: {m}"),
            Self::Failed(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ChatError {}
