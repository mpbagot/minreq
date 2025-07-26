extern crate minreq;
mod setup;

use minreq::TCPConnection;
use self::setup::*;

#[test]
#[cfg(any(feature = "rustls", feature = "https-bundled", feature = "native-tls"))]
fn test_https() {
    // TODO: Implement this locally.
    assert_eq!(
        get_status_code(minreq::get("https://example.com").send()),
        200,
    );
}

#[test]
fn test_timeout_too_low() {
    setup();
    let conn = TCPConnection::new(Some(1));
    let result = minreq::get(url("/slow_a"))
        .with_body("Q".to_string())
        .with_timeout(1)
        .send(conn);
    assert!(result.is_err());
}

#[test]
fn test_timeout_high_enough() {
    setup();
    let conn = TCPConnection::new(Some(3));
    let body = get_body(
        minreq::get(url("/slow_a"))
            .with_body("Q".to_string())
            .with_timeout(3)
            .send(conn),
    );
    assert_eq!(body, "j: Q");
}

#[test]
fn test_headers() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(
        minreq::get(url("/header_pong"))
            .with_header("Ping", "Qwerty")
            .send(conn),
    );
    assert_eq!("Qwerty", body);
}

#[test]
fn test_custom_method() {
    use minreq::Method;
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(
        minreq::Request::new(Method::Custom("GET".to_string()), url("/a"))
            .with_body("Q")
            .send(conn),
    );
    assert_eq!("j: Q", body);
}

#[test]
fn test_get() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::get(url("/a")).with_body("Q").send(conn));
    assert_eq!(body, "j: Q");
}

#[test]
fn test_redirect_get() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::get(url("/redirect")).with_body("Q").send(conn));
    assert_eq!(body, "j: Q");
}

#[test]
fn test_redirect_get_without_following() {
    setup();
    let conn = TCPConnection::new(None);
    let res = minreq::get(url("/redirect"))
        .with_body("Q")
        .with_follow_redirects(false)
        .send(conn)
        .unwrap();
    assert_eq!(res.status_code, 301);
    assert_eq!(
        res.headers.get("location").unwrap(),
        "http://localhost:35562/a",
    );
}

#[test]
fn test_redirect_post() {
    setup();
    let conn = TCPConnection::new(None);
    // POSTing to /redirect should return a 303, which means we should
    // make a GET request to the given location. This test relies on
    // the fact that the test server only responds to GET requests on
    // the /a path.
    let body = get_body(minreq::post(url("/redirect")).with_body("Q").send(conn));
    assert_eq!(body, "j: Q");
}

#[test]
fn test_redirect_with_fragment() {
    setup();
    let conn = TCPConnection::new(None);
    let original_url = url("/redirect#foo");
    let res = minreq::get(original_url).send(conn).unwrap();
    // Fragment should stay the same, otherwise redirected
    assert_eq!(res.url.as_str(), url("/a#foo"));
}

#[test]
fn test_redirect_with_overridden_fragment() {
    setup();
    let conn = TCPConnection::new(None);
    let original_url = url("/redirect-baz#foo");
    let res = minreq::get(original_url).send(conn).unwrap();
    // This redirect should provide its own fragment, overriding the initial one
    assert_eq!(res.url.as_str(), url("/a#baz"));
}

#[test]
fn test_infinite_redirect() {
    setup();
    let conn = TCPConnection::new(None);
    let body = minreq::get(url("/infiniteredirect")).send(conn);
    assert!(body.is_err());
}

#[test]
fn test_relative_redirect_get() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::get(url("/relativeredirect")).with_body("Q").send(conn));
    assert_eq!(body, "j: Q");
}

#[test]
fn test_head() {
    setup();
    let conn = TCPConnection::new(None);
    assert_eq!(get_status_code(minreq::head(url("/b")).send(conn)), 418);
}

#[test]
fn test_post() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::post(url("/c")).with_body("E").send(conn));
    assert_eq!(body, "l: E");
}

#[test]
fn test_put() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::put(url("/d")).with_body("R").send(conn));
    assert_eq!(body, "m: R");
}

#[test]
fn test_delete() {
    setup();
    let conn = TCPConnection::new(None);
    assert_eq!(get_body(minreq::delete(url("/e")).send(conn)), "n: ");
}

#[test]
fn test_trace() {
    setup();
    let conn = TCPConnection::new(None);
    assert_eq!(get_body(minreq::trace(url("/f")).send(conn)), "o: ");
}

#[test]
fn test_options() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::options(url("/g")).with_body("U").send(conn));
    assert_eq!(body, "p: U");
}

#[test]
fn test_connect() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::connect(url("/h")).with_body("I").send(conn));
    assert_eq!(body, "q: I");
}

#[test]
fn test_patch() {
    setup();
    let conn = TCPConnection::new(None);
    let body = get_body(minreq::patch(url("/i")).with_body("O").send(conn));
    assert_eq!(body, "r: O");
}

#[test]
fn tcp_connect_timeout() {
    let conn = TCPConnection::new(Some(1));
    let _listener = std::net::TcpListener::bind("127.0.0.1:32162").unwrap();
    let resp = minreq::Request::new(minreq::Method::Get, "http://127.0.0.1:32162")
        .with_timeout(1)
        .send(conn);
    assert!(resp.is_err());
    // if let Some(minreq::Error::StreamReadError(err)) = resp.err() {
    //     assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    // } else {
    //     panic!("timeout test request did not return an error");
    // }
}

#[test]
fn test_header_cap() {
    setup();
    let conn = TCPConnection::new(None);
    let body = minreq::get(url("/long_header"))
        .with_max_headers_size(999)
        .send(conn);
    assert!(body.is_err());
    assert!(matches!(body.err(), Some(minreq::Error::HeadersOverflow)));

    let conn = TCPConnection::new(None);
    let body = minreq::get(url("/long_header"))
        .with_max_headers_size(1500)
        .send(conn);
    assert!(body.is_ok());
}

#[test]
fn test_status_line_cap() {
    setup();
    let expected_status_line = "HTTP/1.1 203 Non-Authoritative Information";

    let conn = TCPConnection::new(None);
    let body = minreq::get(url("/long_status_line"))
        .with_max_status_line_length(expected_status_line.len() + 1)
        .send(conn);
    assert!(body.is_err());
    assert!(matches!(
        body.err(),
        Some(minreq::Error::StatusLineOverflow)
    ));

    let conn = TCPConnection::new(None);
    let body = minreq::get(url("/long_status_line"))
        .with_max_status_line_length(expected_status_line.len() + 2)
        .send(conn);
    assert!(body.is_ok());
}

#[test]
fn test_massive_content_length() {
    setup();
    let conn = TCPConnection::new(None);
    std::thread::spawn(|| {
        // If minreq trusts Content-Length, this should crash pretty much straight away.
        let _ = minreq::get(url("/massive_content_length")).send(conn);
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    // If it were to crash, it would have at this point. Pass!
}
