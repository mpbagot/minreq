use crate::connection::Connection;
use crate::http_url::{HttpUrl, Port};
#[cfg(feature = "proxy")]
use crate::proxy::Proxy;
use crate::util::VecWriter;
use crate::{Error, Response, ResponseLazy};
#[cfg(not(feature = "tcp"))]
use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
#[cfg(not(feature = "tcp"))]
use core::{fmt, fmt::Write, mem::swap};
#[cfg(feature = "tcp")]
use std::{collections::BTreeMap, fmt, fmt::Write, mem::swap, string::String, vec::Vec};

/// A URL type for requests.
pub type URL = String;

/// An HTTP request method.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Method {
    /// The GET method
    Get,
    /// The HEAD method
    Head,
    /// The POST method
    Post,
    /// The PUT method
    Put,
    /// The DELETE method
    Delete,
    /// The CONNECT method
    Connect,
    /// The OPTIONS method
    Options,
    /// The TRACE method
    Trace,
    /// The PATCH method
    Patch,
    /// A custom method, use with care: the string will be embedded in
    /// your request as-is.
    Custom(String),
}

impl fmt::Display for Method {
    /// Formats the Method to the form in the HTTP request,
    /// ie. Method::Get -> "GET", Method::Post -> "POST", etc.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Method::Get => write!(f, "GET"),
            Method::Head => write!(f, "HEAD"),
            Method::Post => write!(f, "POST"),
            Method::Put => write!(f, "PUT"),
            Method::Delete => write!(f, "DELETE"),
            Method::Connect => write!(f, "CONNECT"),
            Method::Options => write!(f, "OPTIONS"),
            Method::Trace => write!(f, "TRACE"),
            Method::Patch => write!(f, "PATCH"),
            Method::Custom(ref s) => write!(f, "{}", s),
        }
    }
}

/// An HTTP request.
///
/// Generally created by the [`minreq::get`](fn.get.html)-style
/// functions, corresponding to the HTTP method we want to use.
///
/// # Example
///
/// ```
/// let request = minreq::post("http://example.com");
/// ```
///
/// After creating the request, you would generally call
/// [`send`](struct.Request.html#method.send) or
/// [`send_lazy`](struct.Request.html#method.send_lazy) on it, as it
/// doesn't do much on its own.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Request {
    pub(crate) method: Method,
    url: URL,
    params: String,
    headers: BTreeMap<String, String>,
    body: Option<Vec<u8>>,
    pub(crate) timeout: Option<u64>,
    pub(crate) max_headers_size: Option<usize>,
    pub(crate) max_status_line_len: Option<usize>,
    max_redirects: usize,
    pub(crate) follow_redirects: bool,
    #[cfg(feature = "proxy")]
    pub(crate) proxy: Option<Proxy>,
}

impl Request {
    /// Creates a new HTTP `Request`.
    ///
    /// This is only the request's data, it is not sent yet. For
    /// sending the request, see [`send`](struct.Request.html#method.send).
    ///
    /// If `urlencoding` is not enabled, it is the responsibility of the
    /// user to ensure there are no illegal characters in the URL.
    ///
    /// If `urlencoding` is enabled, the resource part of the URL will be
    /// encoded. Any URL special characters (e.g. &, #, =) are not encoded
    /// as they are assumed to be meaningful parameters etc.
    pub fn new<T: Into<URL>>(method: Method, url: T) -> Request {
        Request {
            method,
            url: url.into(),
            params: String::new(),
            headers: BTreeMap::new(),
            body: None,
            timeout: None,
            max_headers_size: None,
            max_status_line_len: None,
            max_redirects: 100,
            follow_redirects: true,
            #[cfg(feature = "proxy")]
            proxy: None,
        }
    }

    /// Add headers to the request this is called on. Use this
    /// function to add headers to your requests.
    pub fn with_headers<T, K, V>(mut self, headers: T) -> Request
    where
        T: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let headers = headers.into_iter().map(|(k, v)| (k.into(), v.into()));
        self.headers.extend(headers);
        self
    }

    /// Adds a header to the request this is called on. Use this
    /// function to add headers to your requests.
    pub fn with_header<T: Into<String>, U: Into<String>>(mut self, key: T, value: U) -> Request {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Sets the request body.
    pub fn with_body<T: Into<Vec<u8>>>(mut self, body: T) -> Request {
        let body = body.into();
        let body_length = body.len();
        self.body = Some(body);
        self.with_header("Content-Length", format!("{}", body_length))
    }

    /// Adds given key and value as query parameter to request url
    /// (resource).
    ///
    /// If `urlencoding` is not enabled, it is the responsibility
    /// of the user to ensure there are no illegal characters in the
    /// key or value.
    ///
    /// If `urlencoding` is enabled, the key and value are both encoded.
    pub fn with_param<T: Into<String>, U: Into<String>>(mut self, key: T, value: U) -> Request {
        let key = key.into();
        #[cfg(feature = "urlencoding")]
        let key = urlencoding::encode(&key);
        let value = value.into();
        #[cfg(feature = "urlencoding")]
        let value = urlencoding::encode(&value);

        if !self.params.is_empty() {
            self.params.push('&');
        }
        self.params.push_str(&key);
        self.params.push('=');
        self.params.push_str(&value);
        self
    }

    /// Sets the request timeout in seconds.
    pub fn with_timeout(mut self, timeout: u64) -> Request {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the max redirects we follow until giving up. 100 by
    /// default.
    ///
    /// Warning: setting this to a very high number, such as 1000, may
    /// cause a stack overflow if that many redirects are followed. If
    /// you have a use for so many redirects that the stack overflow
    /// becomes a problem, please open an issue.
    pub fn with_max_redirects(mut self, max_redirects: usize) -> Request {
        self.max_redirects = max_redirects;
        self
    }

    /// Enables or disables redirect handling. Defaults to `true`, i.e. enabled.
    ///
    /// If `follow` is `true` and the server returns a 301, 302, 303, or 307
    /// status code, minreq will follow the redirect by making another HTTP
    /// request to the new location, up to the amount of times specified by
    /// [`Request::with_max_redirects`], returning an error if that amount is
    /// reached before getting a non-redirection as a response.
    ///
    /// Disabling redirection handling with this function by passing in `false`
    /// can be used to handle the redirects yourself.
    pub fn with_follow_redirects(mut self, follow_redirects: bool) -> Request {
        self.follow_redirects = follow_redirects;
        self
    }

    /// Sets the maximum size of all the headers this request will
    /// accept.
    ///
    /// If this limit is passed, the request will close the connection
    /// and return an [Error::HeadersOverflow] error.
    ///
    /// The maximum length is counted in bytes, including line-endings
    /// and other whitespace. Both normal and trailing headers count
    /// towards this cap.
    ///
    /// `None` disables the cap, and may cause the program to use any
    /// amount of memory if the server responds with a lot of headers
    /// (or an infinite amount). In minreq versions 2.x.x, the default
    /// is None, so setting this manually is recommended when talking
    /// to untrusted servers.
    pub fn with_max_headers_size<S: Into<Option<usize>>>(mut self, max_headers_size: S) -> Request {
        self.max_headers_size = max_headers_size.into();
        self
    }

    /// Sets the maximum length of the status line this request will
    /// accept.
    ///
    /// If this limit is passed, the request will close the connection
    /// and return an [Error::StatusLineOverflow] error.
    ///
    /// The maximum length is counted in bytes, including the
    /// line-ending `\r\n`.
    ///
    /// `None` disables the cap, and may cause the program to use any
    /// amount of memory if the server responds with a long (or
    /// infinite) status line. In minreq versions 2.x.x, the default
    /// is None, so setting this manually is recommended when talking
    /// to untrusted servers.
    pub fn with_max_status_line_length<S: Into<Option<usize>>>(
        mut self,
        max_status_line_len: S,
    ) -> Request {
        self.max_status_line_len = max_status_line_len.into();
        self
    }

    /// Sets the proxy to use.
    #[cfg(feature = "proxy")]
    pub fn with_proxy(mut self, proxy: Proxy) -> Request {
        self.proxy = Some(proxy);
        self
    }

    /// Return the request as a newly allocated u8 vector
    pub fn as_bytes(self) -> Result<Vec<u8>, Error> {
        let parsed_request = ParsedRequest::new(self)?;
        Ok(parsed_request.as_bytes())
    }

    /// Fill a preallocated buffer with the request content.
    /// If the buffer is insufficiently large to hold the request
    /// content, it will be reallocated.
    ///
    /// # Errors
    ///
    /// Returns `Err` if we run into an error while parsing the request
    pub fn fill_buffer(self, buf: &mut Vec<u8>) -> Result<(), Error> {
        let parsed_request = ParsedRequest::new(self)?;
        Ok(parsed_request.fill_buffer(buf))
    }

    /// Sends this request to the host.
    ///
    /// # Errors
    ///
    /// Returns `Err` if we run into an error while sending the
    /// request, or receiving/parsing the response. The specific error
    /// is described in the `Err`, and it can be any
    /// [`minreq::Error`](enum.Error.html) except
    /// [`SerdeJsonError`](enum.Error.html#variant.SerdeJsonError) and
    /// [`InvalidUtf8InBody`](enum.Error.html#variant.InvalidUtf8InBody).
    pub fn send<T: Connection>(self, conn: T) -> Result<Response, Error> {
        let parsed_request = ParsedRequest::new(self)?;
        #[cfg(not(any(feature = "rustls", feature = "openssl", feature = "native-tls")))]
        if parsed_request.url.https {
            return Err(Error::HttpsFeatureNotEnabled);
        }
        let is_head = parsed_request.config.method == Method::Head;
        let response = conn.send(parsed_request)?;
        Response::create(response, is_head)
    }

    /// Sends this request to the host, loaded lazily.
    ///
    /// # Errors
    ///
    /// See [`send`](struct.Request.html#method.send).
    pub fn send_lazy<T: Connection>(self, conn: T) -> Result<ResponseLazy, Error> {
        let parsed_request = ParsedRequest::new(self)?;
        #[cfg(not(any(feature = "rustls", feature = "openssl", feature = "native-tls")))]
        if parsed_request.url.https {
            return Err(Error::HttpsFeatureNotEnabled);
        }
        conn.send(parsed_request)
    }
}

/// A parsed out request. Mainly a wrapper arround Request with a properly parsed url.
/// Required for [`Connection::send()`](trait.Connection.html#method.send) to
/// produce a final ResponseLazy. Should not be constructed manually, but should only be
/// instantiated through Request instance methods.
pub struct ParsedRequest {
    /// The url of the request
    pub url: HttpUrl,
    /// The chain of redirected urls
    pub redirects: Vec<HttpUrl>,
    /// The underlying request object
    pub config: Request,
}

impl ParsedRequest {
    #[allow(unused_mut)]
    fn new(mut config: Request) -> Result<ParsedRequest, Error> {
        let mut url = HttpUrl::parse(&config.url, None)?;

        if !config.params.is_empty() {
            if url.path_and_query.contains('?') {
                url.path_and_query.push('&');
            } else {
                url.path_and_query.push('?');
            }
            url.path_and_query.push_str(&config.params);
        }

        #[cfg(feature = "proxy")]
        // Set default proxy from environment variables
        //
        // Curl documentation: https://everything.curl.dev/usingcurl/proxies/env
        //
        // Accepted variables are `http_proxy`, `https_proxy`, `HTTPS_PROXY`, `ALL_PROXY`
        //
        // Note: https://everything.curl.dev/usingcurl/proxies/env#http_proxy-in-lower-case-only
        if config.proxy.is_none() {
            // Set HTTP proxies if request's protocol is HTTPS and they're given
            if url.https {
                if let Ok(proxy) =
                    std::env::var("https_proxy").map_err(|_| std::env::var("HTTPS_PROXY"))
                {
                    if let Ok(proxy) = Proxy::new(proxy) {
                        config.proxy = Some(proxy);
                    }
                }
            }
            // Set HTTP proxies if request's protocol is HTTP and they're given
            else if let Ok(proxy) = std::env::var("http_proxy") {
                if let Ok(proxy) = Proxy::new(proxy) {
                    config.proxy = Some(proxy);
                }
            }
            // Set any given proxies if neither of HTTP/HTTPS were given
            else if let Ok(proxy) =
                std::env::var("all_proxy").map_err(|_| std::env::var("ALL_PROXY"))
            {
                if let Ok(proxy) = Proxy::new(proxy) {
                    config.proxy = Some(proxy);
                }
            }
        }

        Ok(ParsedRequest {
            url,
            redirects: Vec::new(),
            config,
        })
    }

    fn get_http_head(&self) -> String {
        let mut http = String::with_capacity(32);

        // NOTE: As of 2.10.0, the fragment is intentionally left out of the request, based on:
        // - [RFC 3986 section 3.5](https://datatracker.ietf.org/doc/html/rfc3986#section-3.5):
        //   "...the fragment identifier is not used in the scheme-specific
        //   processing of a URI; instead, the fragment identifier is separated
        //   from the rest of the URI prior to a dereference..."
        // - [RFC 7231 section 9.5](https://datatracker.ietf.org/doc/html/rfc7231#section-9.5):
        //   "Although fragment identifiers used within URI references are not
        //   sent in requests..."

        self.head_to_buf(&mut http);
        http
    }

    fn head_to_buf<T: Write>(&self, http: &mut T) {
        // Add the request line and the "Host" header
        write!(
            http,
            "{} {} HTTP/1.1\r\nHost: {}",
            self.config.method, self.url.path_and_query, self.url.host
        )
        .unwrap();
        if let Port::Explicit(port) = self.url.port {
            write!(http, ":{}", port).unwrap();
        }
        write!(http, "\r\n").unwrap();

        // Add other headers
        for (k, v) in &self.config.headers {
            write!(http, "{}: {}\r\n", k, v).unwrap();
        }

        if self.config.method == Method::Post
            || self.config.method == Method::Put
            || self.config.method == Method::Patch
        {
            let not_length = |key: &String| {
                let key = key.to_lowercase();
                key != "content-length" && key != "transfer-encoding"
            };
            if self.config.headers.keys().all(not_length) {
                // A user agent SHOULD send a Content-Length in a request message when no Transfer-Encoding
                // is sent and the request method defines a meaning for an enclosed payload body.
                // refer: https://tools.ietf.org/html/rfc7230#section-3.3.2

                // A client MUST NOT send a message body in a TRACE request.
                // refer: https://tools.ietf.org/html/rfc7231#section-4.3.8
                // similar line found for GET, HEAD, CONNECT and DELETE.

                write!(http, "Content-Length: 0\r\n").unwrap();
            }
        }

        write!(http, "\r\n").unwrap();
    }

    /// Returns the HTTP request as bytes, ready to be sent to
    /// the server.
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        let mut head = self.get_http_head().into_bytes();
        if let Some(body) = &self.config.body {
            head.extend(body);
        }
        head
    }

    /// Write the HTTP request as bytes to a preallocated buffer
    pub(crate) fn fill_buffer(&self, buf: &mut Vec<u8>) -> () {
        let mut writer_buf = VecWriter::new(buf);
        self.head_to_buf(&mut writer_buf);
        // Now that the header is done, add
        if let Some(body) = &self.config.body {
            buf.extend(body);
        }
    }

    /// Returns the redirected version of this Request, unless an
    /// infinite redirection loop was detected, or the redirection
    /// limit was reached.
    pub(crate) fn redirect_to(&mut self, url: &str) -> Result<(), Error> {
        if url.contains("://") {
            let mut url = HttpUrl::parse(url, Some(&self.url))
                .map_err(|_| Error::InvalidProtocolInRedirect)?;
            swap(&mut url, &mut self.url);
            self.redirects.push(url);
        } else {
            // The url does not have the protocol part, assuming it's
            // a relative resource.
            let mut absolute_url = String::new();
            self.url.write_base_url_to(&mut absolute_url).unwrap();
            absolute_url.push_str(url);
            let mut url = HttpUrl::parse(&absolute_url, Some(&self.url))?;
            swap(&mut url, &mut self.url);
            self.redirects.push(url);
        }

        if self.redirects.len() > self.config.max_redirects {
            Err(Error::TooManyRedirections)
        } else if self
            .redirects
            .iter()
            .any(|redirect_url| redirect_url == &self.url)
        {
            Err(Error::InfiniteRedirectionLoop)
        } else {
            Ok(())
        }
    }
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Get](enum.Method.html).
pub fn get<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Get, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Head](enum.Method.html).
pub fn head<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Head, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Post](enum.Method.html).
pub fn post<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Post, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Put](enum.Method.html).
pub fn put<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Put, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Delete](enum.Method.html).
pub fn delete<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Delete, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Connect](enum.Method.html).
pub fn connect<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Connect, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Options](enum.Method.html).
pub fn options<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Options, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Trace](enum.Method.html).
pub fn trace<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Trace, url)
}

/// Alias for [Request::new](struct.Request.html#method.new) with `method` set to
/// [Method::Patch](enum.Method.html).
pub fn patch<T: Into<URL>>(url: T) -> Request {
    Request::new(Method::Patch, url)
}

#[cfg(test)]
mod parsing_tests {

    use alloc::collections::BTreeMap;
    use alloc::string::ToString;

    use super::{get, ParsedRequest};

    #[test]
    fn test_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("foo".to_string(), "bar".to_string());
        headers.insert("foo".to_string(), "baz".to_string());

        let req = get("http://www.example.org/test/res").with_headers(headers.clone());

        assert_eq!(req.headers, headers);
    }

    #[test]
    fn test_multiple_params() {
        let req = get("http://www.example.org/test/res")
            .with_param("foo", "bar")
            .with_param("asd", "qwe");
        let req = ParsedRequest::new(req).unwrap();
        assert_eq!(&req.url.path_and_query, "/test/res?foo=bar&asd=qwe");
    }

    #[test]
    fn test_domain() {
        let req = get("http://www.example.org/test/res").with_param("foo", "bar");
        let req = ParsedRequest::new(req).unwrap();
        assert_eq!(&req.url.host, "www.example.org");
    }

    #[test]
    fn test_protocol() {
        let req =
            ParsedRequest::new(get("http://www.example.org/").with_param("foo", "bar")).unwrap();
        assert!(!req.url.https);
        let req =
            ParsedRequest::new(get("https://www.example.org/").with_param("foo", "bar")).unwrap();
        assert!(req.url.https);
    }
}

#[cfg(all(test, feature = "urlencoding"))]
mod encoding_tests {
    use super::{get, ParsedRequest};

    #[test]
    fn test_with_param() {
        let req = get("http://www.example.org").with_param("foo", "bar");
        let req = ParsedRequest::new(req).unwrap();
        assert_eq!(&req.url.path_and_query, "/?foo=bar");

        let req = get("http://www.example.org").with_param("ówò", "what's this? 👀");
        let req = ParsedRequest::new(req).unwrap();
        assert_eq!(
            &req.url.path_and_query,
            "/?%C3%B3w%C3%B2=what%27s%20this%3F%20%F0%9F%91%80"
        );
    }

    #[test]
    fn test_on_creation() {
        let req = ParsedRequest::new(get("http://www.example.org/?foo=bar#baz")).unwrap();
        assert_eq!(&req.url.path_and_query, "/?foo=bar");

        let req = ParsedRequest::new(get("http://www.example.org/?ówò=what's this? 👀")).unwrap();
        assert_eq!(
            &req.url.path_and_query,
            "/?%C3%B3w%C3%B2=what%27s%20this?%20%F0%9F%91%80"
        );
    }
}
