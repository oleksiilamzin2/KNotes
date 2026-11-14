#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Cursor;

use knotes_chat::{ChatError, HttpRequest, HttpResponse, Transport};
use serde_json::Value;

/// OpenAI as a list of answers (status, body), keeping what was sent.
#[derive(Default)]
pub struct Fake {
    pub answers: RefCell<VecDeque<Result<(u16, String), ChatError>>>,
    pub sent: RefCell<Vec<HttpRequest>>,
}

impl Fake {
    pub fn answering(answers: Vec<(u16, String)>) -> Self {
        Self { answers: RefCell::new(answers.into_iter().map(Ok).collect()), ..Default::default() }
    }

    pub fn body(&self, i: usize) -> Value {
        serde_json::from_slice(&self.sent.borrow()[i].body).unwrap()
    }

    /// The `i`th request's form fields.
    pub fn form(&self, i: usize) -> Vec<(String, String)> {
        knotes_chat::parse_query(std::str::from_utf8(&self.sent.borrow()[i].body).unwrap())
    }

    pub fn header(&self, i: usize, name: &str) -> String {
        self.sent.borrow()[i].headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or_default()
    }
}

impl Transport for Fake {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, ChatError> {
        self.sent.borrow_mut().push(request.clone());
        let (status, body) = self.answers.borrow_mut().pop_front().expect("an answer left")?;
        Ok(HttpResponse { status, body: Box::new(Cursor::new(body.into_bytes())) })
    }
}
