//! This is a simple example to demonstrate the usage of this library.

fn main() -> Result<(), minreq::Error> {
    let conn = minreq::TLSConnection::new(None);
    let response = minreq::get("https://example.com").send(conn)?;
    let html = response.as_str()?;
    println!("{}", html);
    Ok(())
}
