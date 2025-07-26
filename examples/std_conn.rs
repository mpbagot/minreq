//! This is a simple example to demonstrate the usage of this library.

struct MyConn {
    response_str: std::io::Cursor<Vec<u8>>
}
impl minreq::Connection for MyConn {
    fn send(self, mut request: minreq::ParsedRequest) -> Result<minreq::ResponseLazy, minreq::Error> {
        println!("Request head:\n{}", request.get_http_head());

        // You can wrap anything that implements std::io::Read with CoreReader tto allow it to be
        // directly used with HttpStream without manually reimplementing the read behaviour with
        // CoreRead
        let outstream = minreq::CoreReader(self.response_str);

        let stream = minreq::HttpStream::create_unsecured(outstream, None);
        minreq::ResponseLazy::from_stream(stream, None, None)
    }
}

fn main() -> Result<(), minreq::Error> {
    let conn = MyConn {
        response_str: std::io::Cursor::new("HTTP/1.1 200 OK\r\n\r\nThis is some test response data".to_string().into_bytes())
    };
    let response = minreq::get("http://example.com").send(conn)?;
    println!("Response is:\n{} {} - '{}'\nBody data: '{}'", response.status_code, response.reason_phrase, response.url, response.as_str()?);
    Ok(())
}
