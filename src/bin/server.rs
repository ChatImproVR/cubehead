use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    num::NonZeroUsize,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
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
    msg_buf: AsyncBufferedReceiver,
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
                msg_buf: AsyncBufferedReceiver::new(),
                stream,
                addr,
            });
        }

        let mut live_conns = vec![];

        for mut conn in conns.drain(..) {
            match conn.msg_buf.read(&mut conn.stream)? {
                ReadState::Disconnected => {
                    println!("{} Disconnected", conn.addr);
                }
                ReadState::Complete(buf) => {
                    std::io::stdout().lock().write(&buf).unwrap();
                    live_conns.push(conn);
                }
                ReadState::Invalid | ReadState::Incomplete => {
                    live_conns.push(conn);
                }
            };
        }

        conns = live_conns;

        // Don't spin _too_ fast
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Facilitates reading a little-endian length header, and then a message body over a reliable,
/// asynchronous stream
struct AsyncBufferedReceiver {
    buf: Vec<u8>,
    /// Current position within the buffer
    buf_pos: usize,
}

enum ReadState {
    /// The peer hung up
    Disconnected,
    /// Message incomplete, but the connection is still live
    Incomplete,
    /// Message is complete
    Complete(Vec<u8>),
    /// Invalid message, report error and try again
    Invalid,
}

impl AsyncBufferedReceiver {
    pub fn new() -> Self {
        Self {
            buf: vec![],
            buf_pos: 0,
        }
    }

    /// Read from the given stream without blocking, returning a complete message if any.
    pub fn read<R: Read>(&mut self, mut r: R) -> io::Result<ReadState> {
        // Try to receive a new message if we are not currently processing one
        if self.buf.is_empty() {
            let mut buf = [0u8; 4];
            match r.read(&mut buf) {
                Ok(n_bytes) => {
                    if n_bytes == 0 {
                        return Ok(ReadState::Disconnected);
                    } else if n_bytes == 4 {
                        // Set a new buffer size
                        let msg_size = u32::from_le_bytes(buf);
                        self.buf = vec![0; msg_size as usize];
                        self.buf_pos = 0;
                    } else {
                        return Ok(ReadState::Invalid);
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(ReadState::Incomplete);
                }
                Err(e) => {
                    return Err(e);
                }
            };
        }

        // Attempt to complete the current message
        match r.read(&mut self.buf[self.buf_pos..]) {
            Ok(n_bytes) => {
                if n_bytes == 0 {
                    Ok(ReadState::Disconnected)
                } else {
                    self.buf_pos += n_bytes;
                    if self.buf_pos == self.buf.len() {
                        Ok(ReadState::Complete(std::mem::take(&mut self.buf)))
                    } else {
                        Ok(ReadState::Incomplete)
                    }
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(ReadState::Incomplete),
            Err(e) => Err(e),
        }
    }
}
