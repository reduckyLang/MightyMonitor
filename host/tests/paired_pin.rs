//! A wrong or changed pin on the paired reconnect path is a terminal
//! authentication failure: one attempt, no retry, no plaintext fallback, and no
//! application data or pairing secret reaches the impostor.
use async_trait::async_trait;
use mightymonitor_core::{
    BackoffPolicy, Connector, ReconnectingSession, SessionError, SessionState, SplitMix64,
    Transport, TransportError,
};
use mightymonitor_security::{
    asynchronous::connect_transport,
    pairing::{asynchronous as paired, DeviceIdentity, PairingMode},
    store::{DeviceStore, Peer},
    Endpoint,
};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream};

struct Directory(PathBuf);
impl Directory {
    fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        Self(std::env::temp_dir().join(format!(
            "mm-paired-pin-{label}-{}-{unique}",
            std::process::id()
        )))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Connect(Endpoint);
#[async_trait]
impl Connector for Connect {
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
        connect_transport(&self.0).await
    }
}

async fn sockets() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (client, server) = tokio::join!(
        TcpStream::connect(listener.local_addr().unwrap()),
        listener.accept()
    );
    (client.unwrap(), server.unwrap().0)
}

/// Pair a host and a viewer; returns the viewer's stored view of the host.
async fn pair(
    host: &DeviceIdentity,
    hd: &Directory,
    viewer: &DeviceIdentity,
    vd: &Directory,
) -> Peer {
    let mode = PairingMode::enable();
    let (client, server) = sockets().await;
    let (hpeer, vpeer) = tokio::join!(
        paired::pair_host(server, host, Some(&mode), |_| async { true }),
        paired::pair_client(client, viewer, |_| async { true })
    );
    DeviceStore::open(&hd.0)
        .unwrap()
        .insert(hpeer.unwrap())
        .unwrap();
    let peer = vpeer.unwrap();
    DeviceStore::open(&vd.0)
        .unwrap()
        .insert(peer.clone())
        .unwrap();
    peer
}

fn endpoint(address: SocketAddr, viewer: &Arc<DeviceIdentity>, peer: &Peer) -> Endpoint {
    Endpoint::Paired {
        address,
        identity: viewer.clone(),
        peer: peer.clone(),
    }
}

fn is_auth<T>(result: &Result<T, TransportError>) -> bool {
    matches!(result, Err(TransportError::Authentication(_)))
}

#[tokio::test]
async fn changed_pin_in_stored_credentials_is_terminal_and_never_dials() {
    let (hd, vd) = (Directory::new("h1"), Directory::new("v1"));
    let host = DeviceIdentity::open(&hd.0).unwrap();
    let viewer = Arc::new(DeviceIdentity::open(&vd.0).unwrap());
    let peer = pair(&host, &hd, &viewer, &vd).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    // The caller presents a pin that no longer matches the stored entry.
    let mut changed = peer.clone();
    changed.fingerprint[0] ^= 1;
    let result = connect_transport(&endpoint(address, &viewer, &changed)).await;
    assert!(is_auth(&result));
    // Nothing was dialed: there is no plaintext or unpinned fallback.
    assert!(
        tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wrong_pin_is_terminal_with_one_attempt_and_no_leak() {
    let (hd, vd, ad) = (
        Directory::new("h2"),
        Directory::new("v2"),
        Directory::new("a2"),
    );
    let host = DeviceIdentity::open(&hd.0).unwrap();
    let impostor = DeviceIdentity::open(&ad.0).unwrap();
    let viewer = Arc::new(DeviceIdentity::open(&vd.0).unwrap());
    let peer = pair(&host, &hd, &viewer, &vd).await;
    assert_ne!(impostor.fingerprint(), peer.fingerprint);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let handshakes_completed = Arc::new(AtomicUsize::new(0));
    let (count, completed, adir) = (accepted.clone(), handshakes_completed.clone(), ad.0.clone());
    let server = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            count.fetch_add(1, Ordering::SeqCst);
            // The impostor holds a different key but accepts the TCP connection.
            if paired::accept(stream, &impostor, &adir).await.is_ok() {
                completed.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let result = connect_transport(&endpoint(address, &viewer, &peer)).await;
    assert!(is_auth(&result));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    assert_eq!(handshakes_completed.load(Ordering::SeqCst), 0);
    server.abort();
}

#[tokio::test]
async fn changed_host_key_during_reconnect_gives_up_without_retry_or_fallback() {
    let (hd, vd, ad) = (
        Directory::new("h3"),
        Directory::new("v3"),
        Directory::new("a3"),
    );
    let host = DeviceIdentity::open(&hd.0).unwrap();
    let impostor = DeviceIdentity::open(&ad.0).unwrap();
    let viewer = Arc::new(DeviceIdentity::open(&vd.0).unwrap());
    let peer = pair(&host, &hd, &viewer, &vd).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let impostor_completed = Arc::new(AtomicUsize::new(0));
    let (count, completed) = (accepted.clone(), impostor_completed.clone());
    let (hdir, adir) = (hd.0.clone(), ad.0.clone());
    let server = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                // First connection: the genuine host; drop it to force recovery.
                if let Ok(channel) = paired::accept(stream, &host, &hdir).await {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    drop(channel);
                }
            } else if paired::accept(stream, &impostor, &adir).await.is_ok() {
                // Later connections: the host key changed.
                completed.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    // A policy that would retry quickly and repeatedly if failures were recoverable.
    let policy = BackoffPolicy::default()
        .with_initial(Duration::from_millis(5))
        .with_max(Duration::from_millis(20))
        .with_max_attempts(Some(10));
    let mut session = ReconnectingSession::new(
        Connect(endpoint(address, &viewer, &peer)),
        7,
        policy,
        SplitMix64::new(1),
    );
    session.connect_initial().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), session.recv())
        .await
        .expect("recovery must end promptly");
    assert!(matches!(
        result,
        Err(SessionError::Transport(TransportError::Authentication(_)))
    ));
    assert_eq!(session.state(), SessionState::GaveUp);
    // The session is finished: no more attempts, no fallback transport.
    assert!(matches!(session.recv().await, Err(SessionError::GaveUp)));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        2,
        "one genuine, one rejected attempt"
    );
    assert_eq!(impostor_completed.load(Ordering::SeqCst), 0);
    server.abort();
}
