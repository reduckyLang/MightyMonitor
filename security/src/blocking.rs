//! Deadline-bounded synchronous TLS for Android's networking workers.
use super::*;
use rustls::{pki_types::ServerName, ClientConnection, ServerConnection, StreamOwned};
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
struct DeadlineSocket {
    stream: TcpStream,
    cancelled: Arc<AtomicBool>,
    handshake_end: Option<Instant>,
    write_end: Option<Instant>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}
impl DeadlineSocket {
    fn new(stream: TcpStream, cancelled: Arc<AtomicBool>) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            cancelled,
            handshake_end: Some(Instant::now() + HANDSHAKE_TIMEOUT),
            write_end: None,
            read_timeout: None,
            write_timeout: None,
        })
    }
    fn limit(&self, duration: Option<Duration>, write: bool) -> io::Result<Option<Duration>> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection stopped",
            ));
        }
        let mut result = if self.handshake_end.is_some() {
            Some(Duration::from_millis(100))
        } else {
            duration
        };
        for end in [
            self.handshake_end,
            if write { self.write_end } else { None },
        ]
        .into_iter()
        .flatten()
        {
            let remaining = end.checked_duration_since(Instant::now()).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Secure connection deadline exceeded",
                )
            })?;
            result = Some(result.map_or(remaining, |d| d.min(remaining)));
        }
        Ok(result)
    }
    fn established(&mut self) {
        self.handshake_end = None;
        self.read_timeout = Some(Duration::from_millis(100));
        self.write_timeout = Some(Duration::from_secs(2));
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.stream
                .set_read_timeout(self.limit(self.read_timeout, false)?)?;
            match self.stream.read(bytes) {
                Err(e)
                    if self.handshake_end.is_some()
                        && matches!(
                            e.kind(),
                            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                        ) =>
                {
                    continue
                }
                result => return result,
            }
        }
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.stream
                .set_write_timeout(self.limit(self.write_timeout, true)?)?;
            match self.stream.write(bytes) {
                Err(e)
                    if self.handshake_end.is_some()
                        && matches!(
                            e.kind(),
                            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                        ) =>
                {
                    continue
                }
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.limit(self.write_timeout, true)?;
        self.stream.flush()
    }
}
enum Connection {
    Client(Box<StreamOwned<ClientConnection, DeadlineSocket>>),
    Server(Box<StreamOwned<ServerConnection, DeadlineSocket>>),
}
/// Established TLS channel. Use write_bounded for complete protocol frames;
/// on any failed/interrupted write, close the channel rather than replaying it.
pub struct SecureChannel {
    connection: Connection,
}
impl SecureChannel {
    fn socket(&mut self) -> &mut DeadlineSocket {
        match &mut self.connection {
            Connection::Client(s) => &mut s.sock,
            Connection::Server(s) => &mut s.sock,
        }
    }
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.socket().read_timeout = timeout;
    }
    pub fn write_bounded(&mut self, bytes: &[u8], timeout: Duration) -> io::Result<()> {
        self.socket().write_end = Some(Instant::now() + timeout);
        let result = (|| {
            self.write_all(bytes)?;
            self.flush()
        })();
        self.socket().write_end = None;
        result
    }
}
impl Read for SecureChannel {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match &mut self.connection {
            Connection::Client(s) => s.read(bytes),
            Connection::Server(s) => s.read(bytes),
        }
    }
}
impl Write for SecureChannel {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match &mut self.connection {
            Connection::Client(s) => s.write(bytes),
            Connection::Server(s) => s.write(bytes),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match &mut self.connection {
            Connection::Client(s) => s.flush(),
            Connection::Server(s) => s.flush(),
        }
    }
}
/// The invitation's certificate pin is checked before any viewing token is sent.
/// Certificate chain/name/signature verification remains enabled in Rustls.
pub fn connect(stream: TcpStream, invitation: &Invitation) -> io::Result<SecureChannel> {
    connect_cancellable(stream, invitation, Arc::new(AtomicBool::new(false)))
}
pub fn connect_cancellable(
    stream: TcpStream,
    invitation: &Invitation,
    cancelled: Arc<AtomicBool>,
) -> io::Result<SecureChannel> {
    let mut socket = DeadlineSocket::new(stream, cancelled)?;
    let mut magic = [0; 8];
    socket.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("This endpoint is not a secure Trinix host"));
    }
    let mut length = [0; 2];
    socket.read_exact(&mut length)?;
    let length = usize::from(u16::from_le_bytes(length));
    if length == 0 || length > CERT_LIMIT {
        return Err(invalid("Invalid sharing certificate length"));
    }
    let mut certificate = vec![0; length];
    socket.read_exact(&mut certificate)?;
    let config = invitation.client_config(certificate)?;
    let connection = ClientConnection::new(
        config,
        ServerName::try_from(SERVER_NAME).map_err(tls_error)?,
    )
    .map_err(tls_error)?;
    let mut tls = StreamOwned::new(connection, socket);
    while tls.conn.is_handshaking() {
        tls.conn.complete_io(&mut tls.sock)?;
    }
    if tls.conn.alpn_protocol() != Some(ALPN) {
        return Err(invalid("Secure protocol was not negotiated"));
    }
    tls.write_all(invitation.authentication().as_slice())?;
    tls.flush()?;
    let mut accepted = [0];
    tls.read_exact(&mut accepted)?;
    if accepted != [1] {
        return Err(denied());
    }
    tls.sock.established();
    Ok(SecureChannel {
        connection: Connection::Client(Box::new(tls)),
    })
}
pub fn accept(stream: TcpStream, identity: &ShareIdentity) -> io::Result<SecureChannel> {
    accept_cancellable(stream, identity, Arc::new(AtomicBool::new(false)))
}
pub fn accept_cancellable(
    stream: TcpStream,
    identity: &ShareIdentity,
    cancelled: Arc<AtomicBool>,
) -> io::Result<SecureChannel> {
    let mut socket = DeadlineSocket::new(stream, cancelled)?;
    if identity.certificate.len() > CERT_LIMIT {
        return Err(invalid("Sharing certificate is too large"));
    }
    socket.write_all(MAGIC)?;
    socket.write_all(&(identity.certificate.len() as u16).to_le_bytes())?;
    socket.write_all(identity.certificate.as_ref())?;
    socket.flush()?;
    let connection = ServerConnection::new(identity.config.clone()).map_err(tls_error)?;
    let mut tls = StreamOwned::new(connection, socket);
    while tls.conn.is_handshaking() {
        tls.conn.complete_io(&mut tls.sock)?;
    }
    if tls.conn.alpn_protocol() != Some(ALPN) {
        return Err(invalid("Secure protocol was not negotiated"));
    }
    let mut authentication = Zeroizing::new([0; 40]);
    tls.read_exact(authentication.as_mut())?;
    if !identity.authenticate(&authentication) {
        let _ = tls.write_all(&[0]);
        let _ = tls.flush();
        return Err(denied());
    }
    tls.write_all(&[1])?;
    tls.flush()?;
    tls.sock.established();
    Ok(SecureChannel {
        connection: Connection::Server(Box::new(tls)),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};
    #[test]
    fn stalled_peer_can_be_cancelled_without_waiting_for_handshake_deadline() {
        let identity = ShareIdentity::generate().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let invitation = identity.invitation(listener.local_addr().unwrap()).unwrap();
        let socket = TcpStream::connect(invitation.address()).unwrap();
        let (_silent_peer, _) = listener.accept().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let worker = thread::spawn(move || connect_cancellable(socket, &invitation, signal));
        thread::sleep(Duration::from_millis(50));
        let start = Instant::now();
        cancelled.store(true, Ordering::Relaxed);
        assert!(worker.join().unwrap().is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn pinned_tls_authenticates_and_transfers_payloads() {
        let identity = ShareIdentity::generate().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let invitation = identity.invitation(listener.local_addr().unwrap()).unwrap();
        let address = invitation.address();
        let worker = thread::spawn(move || {
            let mut channel = accept(listener.accept().unwrap().0, &identity).unwrap();
            let mut request = [0; 4];
            channel.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"ping");
            channel
                .write_bounded(b"pong", Duration::from_secs(1))
                .unwrap();
        });
        let mut channel = connect(TcpStream::connect(address).unwrap(), &invitation).unwrap();
        channel
            .write_bounded(b"ping", Duration::from_secs(1))
            .unwrap();
        let mut response = [0; 4];
        channel.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"pong");
        worker.join().unwrap();
    }
    #[test]
    fn wrong_certificate_never_receives_a_token() {
        let real = ShareIdentity::generate().unwrap();
        let attacker = ShareIdentity::generate().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let invitation = real.invitation(listener.local_addr().unwrap()).unwrap();
        let address = invitation.address();
        let worker = thread::spawn(move || {
            assert!(accept(listener.accept().unwrap().0, &attacker).is_err());
        });
        assert!(connect(TcpStream::connect(address).unwrap(), &invitation).is_err());
        worker.join().unwrap();
    }
    #[test]
    fn correct_certificate_with_wrong_token_is_rejected() {
        let identity = ShareIdentity::generate().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut invitation = identity.invitation(listener.local_addr().unwrap()).unwrap();
        invitation.token[0] ^= 1;
        let address = invitation.address();
        let worker = thread::spawn(move || {
            assert!(accept(listener.accept().unwrap().0, &identity).is_err());
        });
        assert!(connect(TcpStream::connect(address).unwrap(), &invitation).is_err());
        worker.join().unwrap();
    }
}
