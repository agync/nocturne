//! The process-wide TLS provider for the HTTP clients that reqwest builds.
//!
//! The binary links one crypto stack, ring, so reqwest is built without a provider of its own
//! and needs one installed before its first client. Build clients here rather than with
//! `reqwest::Client::new()` or `reqwest::Client::builder()`, which panic when none is installed.

use std::sync::Once;

/// Installs ring as the process default provider, once. A provider installed earlier stays.
pub fn ensure_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A client builder with the provider in place.
pub fn builder() -> reqwest::ClientBuilder {
    ensure_provider();
    reqwest::Client::builder()
}

/// A default client with the provider in place.
pub fn client() -> reqwest::Client {
    ensure_provider();
    reqwest::Client::new()
}
