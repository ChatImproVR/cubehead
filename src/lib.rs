use nalgebra::{Matrix4, Point3, UnitQuaternion};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// The position and orientation of a user's head
/// User's head points in the negative Z direction (following OpenGL NDC)
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Default)]
pub struct Head {
    /// Position
    pub pos: Point3<f32>,
    /// Orientation
    pub orient: UnitQuaternion<f32>,
}

impl Head {
    pub fn matrix(&self) -> Matrix4<f32> {
        // TODO: Make this cheaper?
        Matrix4::new_translation(&self.pos.coords) * self.orient.to_homogeneous()
    }
}

/// Facilitates reading a little-endian length header, and then a message body over a reliable,
/// asynchronous stream
pub struct AsyncBufferedReceiver {
    buf: Vec<u8>,
    /// Current position within the buffer
    buf_pos: usize,
}

pub enum ReadState {
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

pub fn serialize_msg<W: Write, T: Serialize>(obj: &T, mut w: W) -> anyhow::Result<()> {
    let size = bincode::serialized_size(obj)?;
    let header = (size as u32).to_le_bytes();
    w.write_all(&header)?;
    Ok(bincode::serialize_into(w, obj)?)
}
