use std::io::BufReader;
use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::{ChatError, HttpRequest, HttpResponse, Method, Transport};

/// HTTP over the system's TLS (Security framework) and its trusted roots.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .tls_config(tls)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(120)))
            .timeout_recv_body(Some(Duration::from_secs(15 * 60)))
            .build();
        Self { agent: ureq::Agent::new_with_config(config) }
    }
}

impl Transport for UreqTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, ChatError> {
        let agent_name = format!("Knotes/{}", env!("CARGO_PKG_VERSION"));
        let response = match request.method {
            Method::Get => {
                let mut builder = self.agent.get(&request.url).header("User-Agent", &agent_name);
                for (key, value) in &request.headers {
                    builder = builder.header(key, value);
                }
                builder.call()
            }
            Method::Post => {
                let mut builder = self.agent.post(&request.url).header("User-Agent", &agent_name);
                for (key, value) in &request.headers {
                    builder = builder.header(key, value);
                }
                builder.send(&request.body[..])
            }
        }
        .map_err(|e| ChatError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        Ok(HttpResponse { status, body: Box::new(BufReader::new(response.into_body().into_reader())) })
    }
}
