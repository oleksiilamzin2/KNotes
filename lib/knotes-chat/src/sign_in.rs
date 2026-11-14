//! Sign in with ChatGPT: the browser, a loopback listener for its return, the code exchange
//! (OpenAI's registration-and-sign-in guide, ADR 0027).

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::crypto::{pkce_challenge, random_token};
use crate::form::{form_encode, parse_query};
use crate::token_endpoint::{AUTHORIZE, PLAN_SCOPE, RESOURCE, SCOPE, TokenFailure, now, post_token};
use crate::{Credentials, Registration, Transport, check_id_token};

/// The client id of a first sign-in, which registers the app.
const REGISTER: &str = "dynamic_agent_client";
const APP_NAME: &str = "Knotes";
const CALLBACK: &str = "/auth/callback";
/// How long the browser has to come back.
const WAIT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInError {
    /// The user turned it down in the browser.
    Declined,
    /// Signed in, but ChatGPT plan use was not granted.
    PlanNotEnabled,
    Cancelled,
    TimedOut,
    Network(String),
    Failed(String),
}

impl fmt::Display for SignInError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Declined => write!(f, "sign-in was declined"),
            Self::PlanNotEnabled => write!(f, "ChatGPT plan use was not allowed"),
            Self::Cancelled => write!(f, "sign-in was cancelled"),
            Self::TimedOut => write!(f, "the browser did not come back"),
            Self::Network(m) => write!(f, "network: {m}"),
            Self::Failed(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for SignInError {}

/// One sign-in attempt, listening for the browser's return from the moment it starts.
pub struct SignIn {
    listener: TcpListener,
    url: String,
    redirect_uri: String,
    state: String,
    nonce: String,
    verifier: String,
    /// The issued client when signing in again; none on the first sign-in.
    client_id: Option<String>,
    /// The account signed in before, which a sign-in again must match.
    subject: Option<String>,
}

impl SignIn {
    /// Starts listening on a free loopback port and prepares the authorization URL. A sign-in
    /// again names the account with its last ID token (`id_token_hint`), when kept.
    pub fn start(registration: &Registration, id_token_hint: Option<&str>) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let redirect_uri = format!("http://127.0.0.1:{}{CALLBACK}", listener.local_addr()?.port());
        let (state, nonce, verifier) = (random_token(), random_token(), random_token());
        let challenge = pkce_challenge(&verifier);
        let client_id = registration.client_id.clone();
        let mut fields = vec![("client_id", client_id.as_deref().unwrap_or(REGISTER))];
        match &client_id {
            None => fields.push(("agent_name_hint", APP_NAME)),
            Some(_) => {
                fields.extend(id_token_hint.map(|h| ("id_token_hint", h)));
                fields.extend(registration.email.as_deref().map(|e| ("login_hint", e)));
            }
        }
        fields.extend([
            ("ext_agent_host_id", registration.host_id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", SCOPE),
            ("resource", RESOURCE),
            ("state", state.as_str()),
            ("nonce", nonce.as_str()),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge.as_str()),
        ]);
        let url = format!("{AUTHORIZE}?{}", form_encode(&fields));
        let subject = client_id.as_ref().and(registration.subject.clone());
        Ok(Self { listener, url, redirect_uri, state, nonce, verifier, client_id, subject })
    }

    /// The page to open in the browser (it carries the ID token hint: not for logs).
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Waits for the browser to come back, exchanges its code and checks what comes.
    pub fn complete(self, transport: &dyn Transport, cancel: &AtomicBool) -> Result<Credentials, SignInError> {
        self.listener.set_nonblocking(true).map_err(|e| SignInError::Failed(e.to_string()))?;
        let started = Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(SignInError::Cancelled);
            }
            if started.elapsed() > WAIT {
                return Err(SignInError::TimedOut);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = self.answer(stream, transport) {
                        return result;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(SignInError::Failed(e.to_string())),
            }
        }
    }

    /// The outcome of one request to the listener; none for a request that is not the
    /// browser's return (another path, a forged `state`).
    fn answer(&self, mut stream: TcpStream, transport: &dyn Transport) -> Option<Result<Credentials, SignInError>> {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let target = request_target(&mut stream)?;
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        if path != CALLBACK {
            respond(&mut stream, "404 Not Found", "Not found");
            return None;
        }
        let fields = parse_query(query);
        let field = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        if field("state") != Some(self.state.as_str()) {
            respond(&mut stream, "400 Bad Request", "Це не той вхід, який почав Knotes.");
            return None;
        }
        let result = match field("error") {
            Some("access_denied") => Err(SignInError::Declined),
            Some(error) => Err(SignInError::Failed(format!("{error}: {}", field("error_description").unwrap_or_default()))),
            None => self.exchange(transport, field("code"), field("client_id"), field("scope")),
        };
        match &result {
            Ok(_) => respond(&mut stream, "200 OK", "Вхід виконано. Можна закрити вкладку й повернутися до Knotes."),
            Err(e) => respond(&mut stream, "200 OK", &format!("Не вдалося увійти ({e}). Поверніться до Knotes.")),
        }
        Some(result)
    }

    fn exchange(&self, transport: &dyn Transport, code: Option<&str>, issued: Option<&str>, scope: Option<&str>) -> Result<Credentials, SignInError> {
        let code = code.ok_or_else(|| SignInError::Failed("no code".into()))?;
        let client_id = match (&self.client_id, issued) {
            (None, Some(issued)) => issued.to_string(),
            (None, None) => return Err(SignInError::Failed("registration did not complete (no client id)".into())),
            (Some(mine), Some(issued)) if mine != issued => return Err(SignInError::Failed("another client came back".into())),
            (Some(mine), _) => mine.clone(),
        };
        let fields = [
            ("grant_type", "authorization_code"),
            ("client_id", client_id.as_str()),
            ("code", code),
            ("code_verifier", self.verifier.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("resource", RESOURCE),
        ];
        let reply = post_token(transport, &fields).map_err(|e| match e {
            TokenFailure::Network(m) => SignInError::Network(m),
            TokenFailure::Unavailable(m) => SignInError::Failed(m),
            TokenFailure::Refused(code, words) => SignInError::Failed(format!("{code}: {words}")),
        })?;
        let id_token = reply.id_token.ok_or_else(|| SignInError::Failed("no ID token".into()))?;
        let claims = check_id_token(&id_token, &client_id, &self.nonce, now()).map_err(|e| SignInError::Failed(format!("ID token: {e}")))?;
        if self.subject.as_ref().is_some_and(|s| *s != claims.subject) {
            return Err(SignInError::Failed("another ChatGPT account signed in".into()));
        }
        let scopes = reply.scopes.unwrap_or_else(|| scope.unwrap_or_default().split_whitespace().map(str::to_string).collect());
        if !scopes.iter().any(|s| s == PLAN_SCOPE) {
            return Err(SignInError::PlanNotEnabled);
        }
        let refresh_token = reply.refresh_token.ok_or_else(|| SignInError::Failed("no refresh token".into()))?;
        Ok(Credentials {
            client_id,
            subject: claims.subject,
            email: claims.email,
            id_token,
            access_token: reply.access_token,
            refresh_token,
            expires_at: now() + reply.expires_in,
            scopes,
        })
    }
}

/// The target of an HTTP GET (`/auth/callback?…`).
fn request_target(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut buffer = [0; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
        let n = stream.read(&mut buffer).ok().filter(|&n| n > 0)?;
        head.extend_from_slice(&buffer[..n]);
    }
    let head = String::from_utf8_lossy(&head);
    let mut words = head.lines().next()?.split_whitespace();
    (words.next()? == "GET").then(|| words.next().map(str::to_string))?
}

fn respond(stream: &mut TcpStream, status: &str, words: &str) {
    let words = words.replace('&', "&amp;").replace('<', "&lt;");
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>Knotes</title><body style=\"font:16px -apple-system,sans-serif;text-align:center;margin-top:20vh\"><h2>Knotes</h2><p>{words}</p>"
    );
    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
    let _ = stream.flush();
}
