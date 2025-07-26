use crate::request::ParsedRequest;
use crate::{Error, Method, ResponseLazy};
#[cfg(not(feature = "tcp"))]
use alloc::{boxed::Box, string::String};
#[cfg(not(feature = "tcp"))]
use core::time::Duration;

#[cfg(feature = "tcp")]
use std::env;
#[cfg(feature = "tcp")]
use std::time::{Duration, Instant};

/// The Connection trait. This implements a specific type of connection over which to send
/// a request.
pub trait Connection {
    /// Send a given request, returning a response or error on failure
    fn send(self, request: ParsedRequest) -> Result<ResponseLazy, Error>;
}

#[cfg(feature = "tcp")]
use std::io::{Read, Write};
#[cfg(feature = "tcp")]
use std::net::{TcpStream, ToSocketAddrs};

/// A replacement implementation of std::io::Read
pub trait CoreRead: Send {
    /// The same as std::io::Read trait implementation, but returns a minreq::Error
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
}
#[cfg(feature = "tcp")]
/// A wrapper struct for unsecured streams to allow them to be used with HTTPStream.
/// In order to use ResponseLazy::from_stream, a HTTPStream instance is needed.
/// To make a HTTPStream, you need to implement the trait CoreRead. If you already
/// have an object that implements std::io::Read, you can wrap it with CoreReader to
/// automatically implement the required CoreRead trait.
pub struct CoreReader<T: std::io::Read>(pub T);
#[cfg(feature = "tcp")]
impl<T: std::io::Read + Send> CoreRead for CoreReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        std::io::Read::read(&mut self.0, buf)
            .map_err(|err: std::io::Error| (Error::StreamReadError(err.to_string())))
    }
}

#[cfg(feature = "rustls")]
mod rustls_stream;
#[cfg(feature = "rustls")]
type SecuredStream = rustls_stream::SecuredStream;

#[cfg(all(not(feature = "rustls"), feature = "native-tls"))]
mod native_tls_stream;
#[cfg(all(not(feature = "rustls"), feature = "native-tls"))]
type SecuredStream = native_tls_stream::SecuredStream;

#[cfg(all(
    not(feature = "rustls"),
    not(feature = "native-tls"),
    feature = "openssl",
))]
mod openssl_stream;
#[cfg(all(
    not(feature = "rustls"),
    not(feature = "native-tls"),
    feature = "openssl",
))]
type SecuredStream = openssl_stream::SecuredStream;

#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
impl CoreRead for SecuredStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        std::io::Read::read(self, buf)
            .map_err(|err: std::io::Error| (Error::StreamReadError(err.to_string())))
    }
}

/// A readable stream of HTTP data. Provided to the
/// [`from_stream()`](struct.ResponseLazy.html#method.from_stream) constructor
/// function to generate a complete HTTP Response object.
pub enum HttpStream {
    /// A wrapped implementation of the CoreRead trait that produces raw HTTP data.
    Unsecured(Box<dyn CoreRead>, Option<Duration>),
    #[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
    /// NOTE: Internal use only for TLS connection implementation
    /// An encrypted readable stream for HTTP data.
    Secured(Box<SecuredStream>, Option<Duration>),
}

impl HttpStream {
    /// Consume an object that implements CoreRead and an optional timeout to create
    /// an unsecured readable HTTPStream for use by ResponseLazy.
    pub fn create_unsecured<T: CoreRead + 'static>(
        reader: T,
        timeout_dur: Option<Duration>,
    ) -> HttpStream {
        HttpStream::Unsecured(Box::new(reader), timeout_dur)
    }

    #[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl"))]
    /// NOTE: Internal use only for TLS connection implementation
    /// Consumes a secured readable stream to produce a readable HTTPStream for use
    /// by ResponseLazy.
    fn create_secured(reader: SecuredStream, timeout_dur: Option<Duration>) -> HttpStream {
        HttpStream::Secured(Box::new(reader), timeout_dur)
    }
}
#[cfg(feature = "tcp")]
impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        CoreRead::read(self, buf)
            .map_err(|err: Error| (std::io::Error::new(std::io::ErrorKind::Other, err.to_string())))
    }
}

#[cfg(feature = "tcp")]
/// Converts a timeout instant to a duration w.r.t the current moment. Only used by the TCP implemention
pub(crate) fn timeout_at_to_duration(
    timeout_at: Option<Instant>,
) -> Result<Option<Duration>, Error> {
    if let Some(timeout_at) = timeout_at {
        if let Some(duration) = timeout_at.checked_duration_since(Instant::now()) {
            Ok(Some(duration))
        } else {
            Err(Error::RequestTimedOut)
        }
    } else {
        Ok(None)
    }
}

trait SetTimeout {
    fn timeout(&self, timeout_dur: Option<Duration>) -> Result<(), Error>;
}
// Default timeout
impl<T: ?Sized + CoreRead> SetTimeout for T {
    fn timeout(&self, _timeout_dur: Option<Duration>) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(feature = "tcp")]
impl SetTimeout for TcpStream {
    fn timeout(&self, timeout_dur: Option<Duration>) -> Result<(), Error> {
        let _ = self.set_read_timeout(timeout_dur);
        Ok(())
    }
}

impl CoreRead for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let result = match self {
            HttpStream::Unsecured(inner, timeout_dur) => {
                inner.timeout(*timeout_dur)?;
                <dyn CoreRead>::read(&mut **inner, buf)
            }
            #[cfg(any(feature = "rustls", feature = "openssl", feature = "native-tls"))]
            HttpStream::Secured(inner, timeout_dur) => {
                inner.get_ref().timeout(*timeout_dur)?;
                // inner.read(buf)
                <dyn CoreRead>::read(&mut **inner, buf)
            }
        };
        match result {
            Err(_e) => Err(Error::RequestTimedOut),
            r => r,
        }
    }
}

/// A connection to the server for sending over http
/// [`Request`](struct.Request.html)s.
#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
pub struct TLSConnection {
    timeout_at: Option<Instant>,
}
#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
impl TLSConnection {
    /// Creates a new `Connection`. See [Request] and [ParsedRequest]
    /// for specifics about *what* is being sent.
    pub fn new(timeout: Option<u64>) -> TLSConnection {
        let timeout = {
            // No env in no_std, so it's either timeout or none
            #[cfg(not(feature = "tcp"))]
            timeout.or_else(None);

            #[cfg(feature = "tcp")]
            timeout.or_else(|| match env::var("MINREQ_TIMEOUT") {
                Ok(t) => t.parse::<u64>().ok(),
                Err(_) => None,
            })
        };
        let timeout_at = timeout.map(|t| Instant::now() + Duration::from_secs(t));
        TLSConnection { timeout_at }
    }
}
#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
impl Connection for TLSConnection {
    /// Sends the [`Request`](struct.Request.html), consumes this
    /// connection, and returns a [`Response`](struct.Response.html).
    #[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
    fn send(self, mut request: ParsedRequest) -> Result<ResponseLazy, Error> {
        enforce_timeout(self.timeout_at, move || {
            request.url.host = ensure_ascii_host(request.url.host)?;

            #[cfg(feature = "rustls")]
            let secured_stream = rustls_stream::create_secured_stream(&self, &request)?;
            #[cfg(all(not(feature = "rustls"), feature = "native-tls"))]
            let secured_stream = native_tls_stream::create_secured_stream(&self, &request)?;
            #[cfg(all(
                not(feature = "rustls"),
                not(feature = "native-tls"),
                feature = "openssl",
            ))]
            let secured_stream = openssl_stream::create_secured_stream(&self, &request)?;

            log::trace!("Reading HTTPS response from {}.", request.url.host);
            let response = ResponseLazy::from_stream(
                secured_stream,
                request.config.max_headers_size,
                request.config.max_status_line_len,
            )?;

            handle_redirects(self, request, response)
        })
    }
}
#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl",))]
impl TCPConn for TLSConnection {
    fn get_conn_timeout(&self) -> Option<Instant> {
        self.timeout_at
    }
}

/// A connection to the server for sending over http
/// [`Request`](struct.Request.html)s.
#[cfg(feature = "tcp")]
pub struct TCPConnection {
    timeout_at: Option<Instant>,
}
#[cfg(feature = "tcp")]
impl TCPConnection {
    /// Creates a new `Connection`. See [Request] and [ParsedRequest]
    /// for specifics about *what* is being sent.
    pub fn new(timeout: Option<u64>) -> TCPConnection {
        let timeout = {
            // No env in no_std, so it's either timeout or none
            #[cfg(not(feature = "tcp"))]
            timeout.or_else(None);

            #[cfg(feature = "tcp")]
            timeout.or_else(|| match env::var("MINREQ_TIMEOUT") {
                Ok(t) => t.parse::<u64>().ok(),
                Err(_) => None,
            })
        };
        let timeout_at = timeout.map(|t| Instant::now() + Duration::from_secs(t));
        TCPConnection { timeout_at }
    }
}
#[cfg(feature = "tcp")]
impl Connection for TCPConnection {
    /// Sends the [`Request`](struct.Request.html), consumes this
    /// connection, and returns a [`Response`](struct.Response.html).
    fn send(self, mut request: ParsedRequest) -> Result<ResponseLazy, Error> {
        enforce_timeout(self.timeout_at, move || {
            request.url.host = ensure_ascii_host(request.url.host)?;
            let bytes = request.as_bytes();

            log::trace!("Establishing TCP connection to {}.", request.url.host);
            let mut tcp = self.connect(&request)?;

            // Send request
            log::trace!("Writing HTTP request.");
            let _ = tcp.set_write_timeout(self.timeout()?);
            tcp.write_all(&bytes)?;

            // Receive response
            log::trace!("Reading HTTP response.");
            let stream = HttpStream::create_unsecured(
                CoreReader(tcp),
                timeout_at_to_duration(self.timeout_at)?,
            );
            let response = ResponseLazy::from_stream(
                stream,
                request.config.max_headers_size,
                request.config.max_status_line_len,
            )?;
            handle_redirects(self, request, response)
        })
    }
}
#[cfg(feature = "tcp")]
impl TCPConn for TCPConnection {
    fn get_conn_timeout(&self) -> Option<Instant> {
        self.timeout_at
    }
}

#[cfg(feature = "tcp")]
trait TCPConn {
    fn get_conn_timeout(&self) -> Option<Instant>;

    /// Returns the timeout duration for operations that should end at
    /// timeout and are starting "now".
    ///
    /// The Result will be Err if the timeout has already passed.
    fn timeout(&self) -> Result<Option<Duration>, Error> {
        let timeout = timeout_at_to_duration(self.get_conn_timeout());
        log::trace!("Timeout requested, it is currently: {:?}", timeout);
        timeout
    }

    fn connect(&self, request: &ParsedRequest) -> Result<TcpStream, Error> {
        let tcp_connect = |host: &str, port: u32| -> Result<TcpStream, Error> {
            let addrs = (host, port as u16)
                .to_socket_addrs()
                .map_err(|err: std::io::Error| Error::StreamReadError(err.to_string()))?;
            let addrs_count = addrs.len();

            // Try all resolved addresses. Return the first one to which we could connect. If all
            // failed return the last error encountered.
            for (i, addr) in addrs.enumerate() {
                let stream = if let Some(timeout) = self.timeout()? {
                    TcpStream::connect_timeout(&addr, timeout)
                } else {
                    TcpStream::connect(addr)
                };
                if stream.is_ok() || i == addrs_count - 1 {
                    return stream.map_err(Error::from);
                }
            }

            Err(Error::AddressNotFound)
        };

        #[cfg(feature = "proxy")]
        match request.config.proxy {
            Some(ref proxy) => {
                // do proxy things
                let mut tcp = tcp_connect(&proxy.server, proxy.port)?;

                write!(tcp, "{}", proxy.connect(&request)).unwrap();
                tcp.flush()?;

                let mut proxy_response = Vec::new();

                loop {
                    let mut buf = vec![0; 256];
                    let total = tcp.read(&mut buf)?;
                    proxy_response.append(&mut buf);
                    if total < 256 {
                        break;
                    }
                }

                crate::Proxy::verify_response(&proxy_response)?;

                Ok(tcp)
            }
            None => tcp_connect(&request.url.host, request.url.port.port()),
        }

        #[cfg(not(feature = "proxy"))]
        tcp_connect(&request.url.host, request.url.port.port())
    }
}

/// Process request redirection, storing the redirection steps in the given ResponseLazy.
/// Returns the completed ResponseLazy with the redirection array on success, or an error
/// on connection read failure.
pub fn handle_redirects<T: Connection>(
    connection: T,
    mut request: ParsedRequest,
    mut response: ResponseLazy,
) -> Result<ResponseLazy, Error> {
    let status_code = response.status_code;
    let url = response.headers.get("location");
    match get_redirect(connection, &mut request, status_code, url) {
        NextHop::Redirect(connection) => {
            let connection = connection?;
            connection.send(request)
        }
        NextHop::Destination(_) => {
            let dst_url = request.url;
            dst_url.write_base_url_to(&mut response.url).unwrap();
            dst_url.write_resource_to(&mut response.url).unwrap();
            Ok(response)
        }
    }
}

enum NextHop<T: Connection> {
    Redirect(Result<T, Error>),
    Destination(Result<T, Error>),
}

fn get_redirect<T: Connection>(
    connection: T,
    request: &mut ParsedRequest,
    status_code: i32,
    url: Option<&String>,
) -> NextHop<T> {
    match status_code {
        301 | 302 | 303 | 307 if request.config.follow_redirects => {
            let url = match url {
                Some(url) => url,
                None => return NextHop::Redirect(Err(Error::RedirectLocationMissing)),
            };
            log::debug!("Redirecting ({}) to: {}", status_code, url);

            match request.redirect_to(url.as_str()) {
                Ok(()) => {
                    if status_code == 303 {
                        match request.config.method {
                            Method::Post | Method::Put | Method::Delete => {
                                request.config.method = Method::Get;
                            }
                            _ => {}
                        }
                    }

                    NextHop::Redirect(Ok(connection))
                }
                Err(err) => NextHop::Redirect(Err(err)),
            }
        }
        _ => NextHop::Destination(Ok(connection)),
    }
}

/// Ensure a given host string is valid ASCII. If it is not, and punycode feature is
/// available, it will be converted to ascii. If punycode feature is disabled, returns
/// a PunycodeFeatureNotEnabled error.
pub fn ensure_ascii_host(host: String) -> Result<String, Error> {
    if host.is_ascii() {
        Ok(host)
    } else {
        #[cfg(not(feature = "punycode"))]
        {
            Err(Error::PunycodeFeatureNotEnabled)
        }

        #[cfg(feature = "punycode")]
        {
            let mut result = String::with_capacity(host.len() * 2);
            for s in host.split('.') {
                if s.is_ascii() {
                    result += s;
                } else {
                    match punycode::encode(s) {
                        Ok(s) => result = result + "xn--" + &s,
                        Err(_) => return Err(Error::PunycodeConversionFailed),
                    }
                }
                result += ".";
            }
            result.truncate(result.len() - 1); // Remove the trailing dot
            Ok(result)
        }
    }
}

/// Enforce the timeout by running the function in a new thread and
/// parking the current one with a timeout.
///
/// While minreq does use timeouts (somewhat) properly, some
/// interfaces such as [ToSocketAddrs] don't allow for specifying the
/// timeout. Hence this.
#[cfg(feature = "tcp")]
fn enforce_timeout<F, R>(timeout_at: Option<Instant>, f: F) -> Result<R, Error>
where
    F: 'static + Send + FnOnce() -> Result<R, Error>,
    R: 'static + Send,
{
    use std::sync::mpsc::{channel, RecvTimeoutError};

    match timeout_at {
        Some(deadline) => {
            let (sender, receiver) = channel();
            let thread = std::thread::spawn(move || {
                let result = f();
                let _ = sender.send(());
                result
            });
            if let Some(timeout_duration) = deadline.checked_duration_since(Instant::now()) {
                match receiver.recv_timeout(timeout_duration) {
                    Ok(()) => thread.join().unwrap(),
                    Err(err) => match err {
                        RecvTimeoutError::Timeout => Err(Error::RequestTimedOut),
                        RecvTimeoutError::Disconnected => {
                            Err(Error::Other("request connection paniced"))
                        }
                    },
                }
            } else {
                Err(Error::RequestTimedOut)
            }
        }
        None => f(),
    }
}
