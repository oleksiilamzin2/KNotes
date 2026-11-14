use std::io::BufRead;

use crate::ChatError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// A response whatever its status; its body read as it comes (a stream of events).
pub struct HttpResponse {
    pub status: u16,
    pub body: Box<dyn BufRead + Send>,
}

/// HTTP to OpenAI: the network in the app, a list of answers in tests.
pub trait Transport {
    /// Fails only when no response came (no network, a timeout).
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, ChatError>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, ChatError> {
        (**self).send(request)
    }
}
