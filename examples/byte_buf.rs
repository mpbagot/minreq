//! This is a simple example to demonstrate the usage of this library for writing out to buffers

fn main() -> Result<(), minreq::Error> {
    let mut buf = Vec::with_capacity(2048);
    let request = minreq::get("http://example.com")
        .with_header("Authorization", "Bearer example-token")
        .with_param("key", "value");
    let _ = request.fill_buffer(&mut buf);
    let req_str = str::from_utf8(&buf).unwrap();
    println!("request: {}", req_str);
    Ok(())
}
