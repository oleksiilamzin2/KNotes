use std::io::Cursor;

use knotes_chat::SseEvents;

#[test]
fn events_are_split_on_blank_lines_their_data_joined_and_comments_skipped() {
    let stream = ": keep-alive\r\nevent: one\r\ndata: {\"a\":\r\ndata: 1}\r\n\r\ndata: two\n\nevent: last\ndata: 3";
    let events: Vec<(String, String)> = SseEvents::new(Cursor::new(stream)).map(|e| e.unwrap()).map(|e| (e.event, e.data)).collect();
    assert_eq!(events, vec![("one".into(), "{\"a\":\n1}".into()), ("".into(), "two".into()), ("last".into(), "3".into())]);
}
