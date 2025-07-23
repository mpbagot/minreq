//! TLS connection handling functionality when using the `openssl` crate for
//! handling TLS.
//!
//! This module is based on how native-tls handles setting up OpenSSL
//! connections. The original native-tls code was provided under the following
//! license:
//!
//! > Copyright (c) 2016 The rust-native-tls Developers
//! >
//! > Permission is hereby granted, free of charge, to any person obtaining a
//! > copy of this software and associated documentation files (the "Software"),
//! > to deal in the Software without restriction, including without limitation
//! > the rights to use, copy, modify, merge, publish, distribute, sublicense,
//! > and/or sell copies of the Software, and to permit persons to whom the
//! > Software is furnished to do so, subject to the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be included in
//! > all copies or substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! > IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! > FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
//! > THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! > LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
//! > FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//! > DEALINGS IN THE SOFTWARE.

use openssl::error::ErrorStack;
use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVersion};
use openssl::x509::X509;
use std::fs;
use std::io::Write;
use std::net::TcpStream;

use crate::Error;

use super::{HttpStream, ParsedRequest, TCPConn, TLSConnection};
use crate::connection::timeout_at_to_duration;

pub type SecuredStream = SslStream<TcpStream>;

impl From<ErrorStack> for Error {
    fn from(err: ErrorStack) -> Self {
        Error::StreamReadError(err.to_string())
    }
}

pub fn create_secured_stream(
    conn: &TLSConnection,
    request: &ParsedRequest,
) -> Result<HttpStream, Error> {
    // openssl setup
    log::trace!("Setting up TLS parameters for {}.", request.url.host);
    let connector = {
        let mut connector_builder = SslConnector::builder(SslMethod::tls())?;
        connector_builder.set_min_proto_version(Some(SslVersion::TLS1))?;

        #[cfg(feature = "openssl-probe")]
        {
            let probe = openssl_probe::probe();
            connector_builder
                .load_verify_locations(probe.cert_file.as_deref(), probe.cert_dir.as_deref())?;
        }

        if cfg!(target_os = "android") {
            if let Ok(dir) = fs::read_dir("/system/etc/security/cacerts") {
                let certs = dir
                    .filter_map(|r| r.ok())
                    .filter_map(|e| fs::read(e.path()).ok())
                    .filter_map(|b| X509::from_pem(&b).ok());
                for cert in certs {
                    if let Err(err) = connector_builder.cert_store_mut().add_cert(cert) {
                        log::debug!("load_android_root_certs error: {:?}", err);
                    }
                }
            }
        }

        connector_builder.build().configure()?
    };

    // Connect
    log::trace!("Establishing TCP connection to {}.", request.url.host);
    let tcp = conn.connect(request)?;

    // Send request
    log::trace!("Establishing TLS session to {}.", request.url.host);
    let mut tls = match connector
        .use_server_name_indication(true)
        .verify_hostname(true)
        .connect(&request.url.host, tcp)
    {
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
