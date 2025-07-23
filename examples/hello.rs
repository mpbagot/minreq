//! This is a simple example to demonstrate the usage of this library.

fn main() -> Result<(), minreq::Error> {
    let conn = minreq::TCPConnection::new(Some(1000));
    let response = minreq::get("http://example.com").send(conn)?;
    println!("{}", response.as_str()?);
    Ok(())
}
