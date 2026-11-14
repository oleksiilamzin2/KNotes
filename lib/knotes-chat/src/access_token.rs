use crate::ChatError;

/// The signed-in user's access token, renewed when OpenAI refuses it.
pub trait AccessToken {
    fn token(&self) -> Result<String, ChatError>;
    /// A new token after a 401; `SignInAgain` when the refresh token is no good.
    fn renew(&self) -> Result<String, ChatError>;
}
