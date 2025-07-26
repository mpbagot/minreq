//! This is a simple example to demonstrate the usage of this library.

struct MyStream {
    data: Vec<u8>,
    index: usize
}
impl minreq::CoreRead for MyStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, minreq::Error> {
        let max_len = self.data.len() - self.index;

        let len = core::cmp::min(max_len, buf.len());
        buf[..len].copy_from_slice(&self.data[..len]);

        self.index += len;
        Ok(len)
    }
}

struct MyConn {}
impl minreq::Connection for MyConn {
    fn send(self, mut request: minreq::ParsedRequest) -> Result<minreq::ResponseLazy, minreq::Error> {
        println!("Request head:\n{}", request.get_http_head());

        let outstream = MyStream {
            data: "HTTP/1.1 200 OK\r\n\r\nThis is some test response data".to_string().into_bytes(),
            index: 0
        };
        let stream = minreq::HttpStream::create_unsecured(outstream, None);
        minreq::ResponseLazy::from_stream(stream, None, None)
    }
}

fn main() -> Result<(), minreq::Error> {
    let conn = MyConn {};
    let response = minreq::get("http://example.com").send(conn)?;
    println!("Response is:\n{} {} - '{}'\nBody data: '{}'", response.status_code, response.reason_phrase, response.url, response.as_str()?);
    Ok(())
}
