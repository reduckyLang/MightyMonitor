//! Bounded complete-frame writes for plaintext development and pinned TLS links.
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
use trinix_security::blocking::SecureChannel;
pub enum Connection {
    Plain(TcpStream),
    Secure(SecureChannel),
}
pub trait FrameWriter: Write {
    fn write_frame(&mut self, bytes: &[u8], timeout: Duration) -> io::Result<()>;
}
impl FrameWriter for TcpStream {
    fn write_frame(&mut self, mut bytes: &[u8], timeout: Duration) -> io::Result<()> {
        let end = Instant::now() + timeout;
        while !bytes.is_empty() {
            let remaining = end
                .checked_duration_since(Instant::now())
                .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Frame write timed out"))?;
            self.set_write_timeout(Some(remaining))?;
            let written = self.write(bytes)?;
            if written == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[written..];
        }
        self.flush()
    }
}
impl Connection {
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.set_read_timeout(timeout),
            Self::Secure(s) => {
                s.set_read_timeout(timeout);
                Ok(())
            }
        }
    }
}
impl Read for Connection {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(bytes),
            Self::Secure(s) => s.read(bytes),
        }
    }
}
impl Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(bytes),
            Self::Secure(s) => s.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Secure(s) => s.flush(),
        }
    }
}
impl FrameWriter for Connection {
    fn write_frame(&mut self, bytes: &[u8], timeout: Duration) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.write_frame(bytes, timeout),
            Self::Secure(s) => s.write_bounded(bytes, timeout),
        }
    }
}
