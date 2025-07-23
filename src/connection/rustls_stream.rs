//! TLS connection handling functionality when using the `rustls` crate for
//! handling TLS.

use rustls::{self, ClientConfig, ClientConnection, RootCertStore, ServerName, StreamOwned};
use std::convert::TryFrom;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;
#[cfg(feature = "rustls-webpki")]
use webpki_roots::TLS_SERVER_ROOTS;

use crate::Error;

use super::{HttpStream, ParsedRequest, TCPConn, TLSConnection};
use crate::connection::timeout_at_to_duration;

pub type SecuredStream = StreamOwned<ClientConnection, TcpStream>;

static CONFIG: std::sync::LazyLock<Arc<ClientConfig>> = std::sync::LazyLock::new(|| {
    let mut root_certificates = RootCertStore::empty();

    // Try to load native certs
    #[cfg(feature = "https-rustls-probe")]
    if let Ok(os_roots) = rustls_native_certs::load_native_certs() {
        for root_cert in os_roots {
            // Ignore erroneous OS certificates, there's nothing
            // to do differently in that situation anyways.
            let _ = root_certificates.add(&rustls::Certificate(root_cert.0));
        }
    }

    #[cfg(feature = "rustls-webpki")]
    #[allow(deprecated)] // Need to use add_server_trust_anchors to compile with rustls 0.21.1
    root_certificates.add_server_trust_anchors(TLS_SERVER_ROOTS.iter().map(|ta| {
        rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));

    let config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_certificates)
        .with_no_client_auth();
    Arc::new(config)
});

pub fn create_secured_stream(
    conn: &TLSConnection,
    request: &ParsedRequest,
) -> Result<HttpStream, Error> {
    // Rustls setup
    log::trace!("Setting up TLS parameters for {}.", request.url.host);
    let dns_name = match ServerName::try_from(&*request.url.host) {
        Ok(result) => result,
        Err(err) => return Err(Error::StreamReadError(err.to_string())),
    };
    let sess =
        ClientConnection::new(CONFIG.clone(), dns_name).map_err(Error::RustlsCreateConnection)?;

    // Connect
    log::trace!("Establishing TCP connection to {}.", request.url.host);
    let tcp = conn.connect(request)?;

    // Send request
    log::trace!("Establishing TLS session to {}.", request.url.host);
    let mut tls = StreamOwned::new(sess, tcp); // I don't think this actually does any communication.
    log::trace!("Writing HTTPS request to {}.", request.url.host);
    let _ = tls.get_ref().set_write_timeout(conn.timeout()?);
    tls.write_all(&request.as_bytes())?;

    Ok(HttpStream::create_secured(
        tls,
        timeout_at_to_duration(conn.timeout_at)?,
    ))
}
