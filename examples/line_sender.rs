use std::{net::{SocketAddr, TcpStream}, io::{self, Write}};

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let addr: SocketAddr = args.next().expect("Requires addr").parse().unwrap();

    let mut stream = TcpStream::connect(addr)?;

    for line in std::io::stdin().lines() {
        let line = line?;

        let header = (line.len() as u32).to_le_bytes();
        stream.write_all(&header)?;
        stream.write_all(&line.as_bytes())?;
    }

    Ok(())
}
