//! Over the network: `cargo test -p knotes-chat --test live -- --ignored`.

use std::io::Read;

use knotes_chat::{HttpRequest, Method, Transport, UreqTransport};

#[test]
#[ignore = "needs the network"]
fn the_network_transport_reaches_openai_over_the_system_s_tls() {
    let request = HttpRequest { method: Method::Get, url: "https://api.openai.com/v1/models".into(), headers: vec![], body: vec![] };
    let mut response = UreqTransport::default().send(&request).unwrap();
    let mut body = String::new();
    response.body.read_to_string(&mut body).unwrap();
    assert_eq!(response.status, 401, "{body}");
    assert!(body.contains("error"), "{body}");
}
