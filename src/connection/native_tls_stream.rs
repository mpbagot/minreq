//! TLS connection handling functionality when using the `native-tls` crate for
//! handling TLS.

use native_tls::{TlsConnector, TlsStream};
use std::io::Write;
use std::net::TcpStream;

use crate::Error;

use super::{HttpStream, ParsedRequest, TCPConn, TLSConnection};
use crate::connection::timeout_at_to_duration;

pub type SecuredStream = TlsStream<TcpStream>;

pub fn create_secured_stream(
    conn: &TLSConnection,
    request: &ParsedRequest,
) -> Result<HttpStream, Error> {
    // native-tls setup
    log::trace!("Setting up TLS parameters for {}.", request.url.host);
    let dns_name = &request.url.host;
    let sess = match TlsConnector::new() {
        Ok(sess) => sess,
        Err(err) => return Err(Error::StreamReadError(err.to_string())),
    };

    // Connect
    log::trace!("Establishing TCP connection to {}.", request.url.host);
    let tcp = conn.connect(request)?;

    // Send request
    log::trace!("Establishing TLS session to {}.", request.url.host);
    let mut tls = match sess.connect(dns_name, tcp) {
        Ok(tls) => tls,
        Err(err) => return Err(Error::StreamReadError(err.to_string())),
    };
    log::trace!("Writing HTTPS request to {}.", request.url.host);
    let _ = tls.get_ref().set_write_timeout(conn.timeout()?);
    tls.write_all(&request.as_bytes())?;

    Ok(HttpStream::create_secured(
        tls,
        timeout_at_to_duration(conn.timeout_at)?,
    ))
}
