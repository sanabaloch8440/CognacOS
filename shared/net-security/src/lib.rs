//! Shared, memory-safe networking and TLS building blocks for CognacOS user mode.
//! This is not a Windows ABI layer; each DLL must validate Windows arguments,
//! preserve Windows structure layouts and error semantics, and export its own ABI.
#![deny(unsafe_op_in_unsafe_fn)]

pub use hyper;
pub use reqwest;
pub use rustls;
pub use socket2;
pub use tokio;
pub use tokio_rustls;
pub use x509_parser;

/// TLS client helpers. Mozilla roots are bootstrap roots, not Windows stores.
pub mod tls {
    use std::sync::Arc;
    use rustls::{ClientConfig, RootCertStore};

    /// Install AWS-LC once. An already-installed provider is accepted.
    pub fn install_crypto_provider() -> Result<(), &'static str> {
        if rustls::crypto::CryptoProvider::get_default().is_some() {
            return Ok(());
        }
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .map_err(|_| "a different rustls crypto provider is already installed")
    }

    /// TLS 1.2+ client config with bundled public trust anchors.
    /// Callers must provide a valid DNS/IP ServerName; verification stays enabled.
    pub fn client_config() -> Result<Arc<ClientConfig>, &'static str> {
        install_crypto_provider()?;
        let roots = RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
        );
        let mut config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        // Keep 0-RTT disabled until replay policy is implemented.
        config.enable_early_data = false;
        Ok(Arc::new(config))
    }
}

/// Parse one DER X.509 certificate and reject trailing bytes.
pub fn parse_x509_der(
    der: &[u8],
) -> Result<x509_parser::certificate::X509Certificate<'_>, x509_parser::prelude::X509Error> {
    let (remaining, certificate) = x509_parser::parse_x509_certificate(der)?;
    if !remaining.is_empty() {
        return Err(x509_parser::prelude::X509Error::InvalidCertificate);
    }
    Ok(certificate)
}
