use std::{
    io::{self, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use cubehead::{Head, AsyncBufferedReceiver, ReadState};

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
    msg_buf: AsyncBufferedReceiver,
}

fn server(conn_rx: Receiver<(TcpStream, SocketAddr)>) -> io::Result<()> {
    let mut conns: Vec<Connection> = vec![];

    loop {
        // Check for new connections
        for (stream, addr) in conn_rx.try_iter() {
            stream.set_nonblocking(true)?;
            eprintln!("{} Connected", addr);
            conns.push(Connection {
                last_pos: Head::default(),
                msg_buf: AsyncBufferedReceiver::new(),
                stream,
                addr,
            });
        }

        let mut live_conns = vec![];

        // Update head positions
        for mut conn in conns.drain(..) {
            match conn.msg_buf.read(&mut conn.stream)? {
                ReadState::Disconnected => {
                    eprintln!("{} Disconnected", conn.addr);
                }
                ReadState::Complete(buf) => {
                    let new_head = bincode::deserialize(&buf).expect("Malformed message");
                    conn.last_pos = new_head;
                    live_conns.push(conn);
                }
                ReadState::Invalid | ReadState::Incomplete => {
                    live_conns.push(conn);
                }
            };
        }

        conns = live_conns;

        // Compile head position message
        let heads: Vec<Head> = conns.iter().map(|c| c.last_pos).collect();
        let msg = bincode::serialize(&heads).unwrap();

        for conn in &mut conns {
            // TODO: Exclude the user's own head! Lmao
            let header = (msg.len() as u32).to_le_bytes();
            conn.stream.write_all(&header)?;
            conn.stream.write_all(&msg)?;
        }

        // Don't spin _too_ fast
        //std::thread::sleep(Duration::from_millis(1));
    }
}
