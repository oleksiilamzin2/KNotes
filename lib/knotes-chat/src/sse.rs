//! Server-sent events, read line by line as they come.

use std::io::{self, BufRead};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, empty when not given.
    pub event: String,
    /// The `data:` lines, joined with line breaks.
    pub data: String,
}

pub struct SseEvents<R> {
    reader: R,
    line: String,
}

impl<R: BufRead> SseEvents<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, line: String::new() }
    }
}

impl<R: BufRead> Iterator for SseEvents<R> {
    type Item = io::Result<SseEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut event = SseEvent { event: String::new(), data: String::new() };
        let (mut fields, mut data_lines) = (false, 0);
        loop {
            self.line.clear();
            match self.reader.read_line(&mut self.line) {
                Err(e) => return Some(Err(e)),
                // A last event without its blank line still counts.
                Ok(0) => return fields.then_some(Ok(event)),
                Ok(_) => {}
            }
            let line = self.line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if fields {
                    return Some(Ok(event));
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = line.split_once(':').map(|(f, v)| (f, v.strip_prefix(' ').unwrap_or(v))).unwrap_or((line, ""));
            match field {
                "event" => event.event = value.to_string(),
                "data" => {
                    if data_lines > 0 {
                        event.data.push('\n');
                    }
                    event.data.push_str(value);
                    data_lines += 1;
                }
                _ => continue,
            }
            fields = true;
        }
    }
}
