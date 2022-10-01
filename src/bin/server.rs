use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::mpsc::{self, Receiver, Sender}, time::Duration,
};

use cubehead::Head;

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let bind_addr = args.next().unwrap_or("127.0.0.1:5031".into());
    let bind_addr: SocketAddr = bind_addr.parse().expect("Failed to parse bind addr");

    // Create a new thread for the connection listener
    let (conn_tx, conn_rx) = mpsc::channel();
    std::thread::spawn(move || connection_listener(bind_addr, conn_tx));

    server(conn_rx)
}

/// Thread which listens for new connections and sends them to the given MPSC channel
/// Technically we could use a non-blocking connection accepter, but it was easier not to for now
fn connection_listener(
    addr: SocketAddr,
    conn_tx: Sender<(TcpStream, SocketAddr)>,
) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    loop {
        conn_tx.send(listener.accept()?).unwrap();
    }
}

struct Connection {
    last_pos: Head,
    stream: TcpStream,
    addr: SocketAddr,
}

fn server(conn_rx: Receiver<(TcpStream, SocketAddr)>) -> io::Result<()> {
    let mut conns: Vec<Connection> = vec![];

    loop {
        // Check for new connections
        for (stream, addr) in conn_rx.try_iter() {
            stream.set_nonblocking(true)?;
            println!("{} connected", addr);
            conns.push(Connection {
                last_pos: Head::default(),
                stream,
                addr,
            });
        }

        let mut live_conns = vec![];

        for mut conn in conns.drain(..) {
            let mut buf = vec![0; 100];
            match conn.stream.read(&mut buf) {
                Ok(n_bytes) => {
                    if n_bytes == 0 {
                        println!("{} disconnected", conn.addr);
                    } else {
                        std::io::stdout().lock().write(&buf[..n_bytes]).unwrap();
                        live_conns.push(conn);
                    }
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    live_conns.push(conn);
                }
                Err(e) => {
                    eprintln!("{} error", conn.addr);
                    dbg!(e);
                }
            };
        }

        conns = live_conns;

        // Don't spin _too_ fast
        std::thread::sleep(Duration::from_millis(1));
    }
}
