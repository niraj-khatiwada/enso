//! The HTTP client GPUI fetches web images with (`https://…` in a note).
//! GPUI starts with one that refuses every request; this one uses the
//! system's TLS and trusted certificates, fetches on a background thread,
//! and can be switched off in Settings.

use bytes::Bytes;
use futures::future::BoxFuture;
use gpui_kit::http_client::{
    self, AsyncBody, HttpClient, Url,
    http::{HeaderValue, Request, Response},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// The largest image enso downloads.
const LIMIT: u64 = 25 * 1024 * 1024;

/// Whether web images load (Settings › Editor › Load Web Images).
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub struct Client {
    agent: ureq::Agent,
    user_agent: HeaderValue,
    /// The switch this client obeys: the Settings one in the app.
    enabled: &'static AtomicBool,
}

impl Client {
    pub fn new() -> Self {
        Self::with_switch(&ENABLED)
    }

    fn with_switch(enabled: &'static AtomicBool) -> Self {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(Duration::from_secs(20)))
            .https_only(false)
            .build()
            .new_agent();
        let version = env!("CARGO_PKG_VERSION");
        Self {
            agent,
            user_agent: HeaderValue::from_str(&format!("enso/{version}"))
                .unwrap_or_else(|_| HeaderValue::from_static("enso")),
            enabled,
        }
    }
}

impl HttpClient for Client {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(&self, request: Request<AsyncBody>) -> BoxFuture<'static, http_client::Result<Response<AsyncBody>>> {
        let agent = self.agent.clone();
        let user_agent = self.user_agent.clone();
        let uri = request.uri().to_string();
        let enabled = self.enabled.load(Ordering::Relaxed);
        Box::pin(async move {
            if !enabled {
                return Err(http_client::anyhow!("Web images are turned off in Settings."));
            }
            if !(uri.starts_with("https://") || uri.starts_with("http://")) {
                return Err(http_client::anyhow!("Only web addresses are fetched: {uri}"));
            }
            // ureq blocks; the window's executor must not.
            let (tx, rx) = futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(fetch(&agent, &uri, &user_agent));
            });
            rx.await.map_err(|_| http_client::anyhow!("The download stopped."))?
        })
    }
}

/// Download `uri` (blocking: call it off the window's thread), within the
/// same limits as images shown in notes. The bytes and their content type.
pub fn download(uri: &str) -> http_client::Result<(Vec<u8>, Option<String>)> {
    if !ENABLED.load(Ordering::Relaxed) {
        return Err(http_client::anyhow!("Web images are turned off in Settings."));
    }
    let client = Client::new();
    let mut response = client
        .agent
        .get(uri)
        .header("User-Agent", client.user_agent.to_str().unwrap_or("enso"))
        .call()
        .map_err(|e| http_client::anyhow!("{e}"))?;
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| http_client::anyhow!("{e}"))?;
    Ok((bytes, content_type))
}

fn fetch(agent: &ureq::Agent, uri: &str, user_agent: &HeaderValue) -> http_client::Result<Response<AsyncBody>> {
    let mut response = agent
        .get(uri)
        .header("User-Agent", user_agent.to_str().unwrap_or("enso"))
        .call()
        .map_err(|e| http_client::anyhow!("{e}"))?;
    let status = response.status();
    let content_type = response.headers().get("content-type").cloned();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| http_client::anyhow!("{e}"))?;
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    builder
        .body(AsyncBody::from_bytes(Bytes::from(bytes)))
        .map_err(|e| http_client::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turned_off_means_no_request() {
        static OFF: AtomicBool = AtomicBool::new(false);
        let request = Request::get("https://example.com/a.png").body(AsyncBody::empty()).unwrap();
        let result = futures::executor::block_on(Client::with_switch(&OFF).send(request));
        match result {
            Err(error) => assert!(error.to_string().contains("turned off")),
            Ok(_) => panic!("a request went out with web images off"),
        }
    }

    /// Needs the network; run with `--ignored`.
    #[test]
    #[ignore]
    fn fetches_a_web_image_over_system_tls() {
        let request = Request::get("https://www.rust-lang.org/logos/rust-logo-128x128.png")
            .body(AsyncBody::empty())
            .unwrap();
        let response = futures::executor::block_on(Client::new().send(request)).unwrap();
        assert!(response.status().is_success());
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        futures::executor::block_on(futures::AsyncReadExt::read_to_end(&mut body, &mut bytes)).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"), "{} bytes", bytes.len());
    }
}
