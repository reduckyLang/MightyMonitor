//! Per-share pairing lifecycle. Discovery is public routing metadata only.
use trinix_security::{
    managed::Command,
    pairing::{asynchronous::pair_host, DeviceIdentity, PairingMode, PAIRING_WINDOW},
    store::DeviceStore,
};
use std::{
    io,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::oneshot;
struct Window {
    mode: Arc<PairingMode>,
    end: Instant,
    decision: Option<oneshot::Sender<bool>>,
    code: Option<String>,
    active: bool,
}
pub struct SharePairing {
    identity: Arc<DeviceIdentity>,
    directory: PathBuf,
    address: SocketAddr,
    window: Mutex<Window>,
}
impl SharePairing {
    pub fn new(
        identity: Arc<DeviceIdentity>,
        directory: PathBuf,
        address: SocketAddr,
    ) -> io::Result<Self> {
        let this = Self {
            identity,
            directory,
            address,
            window: Mutex::new(Window {
                mode: Arc::new(PairingMode::enable()),
                end: Instant::now() + PAIRING_WINDOW,
                decision: None,
                code: None,
                active: false,
            }),
        };
        this.publish_offer()?;
        Ok(this)
    }
    pub fn metadata(&self) -> io::Result<trinix_discovery::txt::Metadata> {
        trinix_discovery::txt::Metadata::new(
            self.identity.friendly_name(),
            &self.identity.id(),
        )
    }
    fn publish_offer(&self) -> io::Result<()> {
        let window = self
            .window
            .lock()
            .map_err(|_| io::Error::other("Pairing unavailable"))?;
        let offer = window.mode.create_offer(&self.identity, self.address)?;
        // Explicit local presentation channel; never diagnostics or discovery.
        println!("MM_PAIR_OFFER {}", offer.to_offer_string());
        println!("MM_PAIR OPEN 60");
        Ok(())
    }
    pub fn command(&self, command: Command) -> io::Result<()> {
        match command {
            Command::Confirm | Command::Deny => {
                let decision = self
                    .window
                    .lock()
                    .map_err(|_| io::Error::other("Pairing unavailable"))?
                    .decision
                    .take();
                if let Some(decision) = decision {
                    let _ = decision.send(command == Command::Confirm);
                }
            }
            Command::Renew => {
                let mut window = self
                    .window
                    .lock()
                    .map_err(|_| io::Error::other("Pairing unavailable"))?;
                if window.mode.locked() || Instant::now() >= window.end {
                    *window = Window {
                        mode: Arc::new(PairingMode::enable()),
                        end: Instant::now() + PAIRING_WINDOW,
                        decision: None,
                        code: None,
                        active: false,
                    };
                    drop(window);
                    self.publish_offer()?;
                }
            }
            Command::Forget(id) => {
                DeviceStore::open(&self.directory)?.unpair(&id)?;
            }
            Command::Stop => {}
        }
        Ok(())
    }
    pub fn record_failure(&self) {
        if let Ok(mut window) = self.window.lock() {
            window.mode.record_failure();
            if window.mode.locked() {
                if let Some(decision) = window.decision.take() {
                    let _ = decision.send(false);
                }
            }
        }
    }
    pub fn publish_status(&self) {
        if let Ok(window) = self.window.lock() {
            if window.decision.is_some() {
                return;
            }
            if window.mode.locked() {
                println!("MM_PAIR LOCKED");
            } else if Instant::now() >= window.end {
                println!("MM_PAIR EXPIRED");
            } else {
                println!(
                    "MM_PAIR OPEN {}",
                    window
                        .end
                        .saturating_duration_since(Instant::now())
                        .as_secs()
                        .min(60)
                );
            }
        }
    }
    pub async fn pair(&self, stream: tokio::net::TcpStream) -> io::Result<()> {
        let mode = {
            let mut window = self
                .window
                .lock()
                .map_err(|_| io::Error::other("Pairing unavailable"))?;
            if window.active {
                window.mode.record_failure();
                if window.mode.locked() {
                    if let Some(decision) = window.decision.take() {
                        let _ = decision.send(false);
                    }
                }
                return Ok(());
            }
            window.active = true;
            window.mode.clone()
        };
        let confirmation_mode = mode.clone();
        let peer = pair_host(stream, &self.identity, Some(&mode), |code| async move {
            let (tx, rx) = oneshot::channel();
            {
                let Ok(mut window) = self.window.lock() else {
                    return false;
                };
                if !Arc::ptr_eq(&window.mode, &confirmation_mode) || window.decision.is_some() {
                    return false;
                }
                window.code = Some(code.clone());
                window.decision = Some(tx);
                println!("MM_PAIR_CODE {code}");
            }
            rx.await.unwrap_or(false)
        })
        .await;
        let mut window = self
            .window
            .lock()
            .map_err(|_| io::Error::other("Pairing unavailable"))?;
        // A renewed window cannot persist or confirm an earlier attempt.
        if !Arc::ptr_eq(&window.mode, &mode) {
            return Err(io::Error::other("Pairing expired"));
        }
        window.decision = None;
        window.code = None;
        window.active = false;
        if let Ok(peer) = peer {
            DeviceStore::open(&self.directory)?.insert(peer)?;
        }
        drop(window);
        self.publish_status();
        Ok(())
    }
}
/// Device endpoint has a bounded routing preface before the existing pinned TLS
/// handshakes. Private-link sharing retains its separate, unchanged endpoint.
pub async fn route(stream: &mut tokio::net::TcpStream) -> io::Result<bool> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        stream.write_all(b"MMROUTE1").await?;
        match stream.read_u8().await? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(io::Error::other("Invalid device request")),
        }
    })
    .await
    .map_err(|_| io::Error::other("Device request timed out"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use trinix_security::pairing::{asynchronous, Clock, PairingOffer, SystemClock};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::{TcpListener, TcpStream};
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "mm-share-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    async fn connection(listener: &TcpListener, pair: bool) -> (TcpStream, TcpStream) {
        let (client, server) = tokio::join!(
            trinix_viewer::devices::dial(listener.local_addr().unwrap(), pair),
            async {
                let mut stream = listener.accept().await.unwrap().0;
                assert_eq!(route(&mut stream).await.unwrap(), pair);
                stream
            }
        );
        (client.unwrap(), server)
    }
    async fn wait_code(service: &SharePairing) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if let Some(code) = service.window.lock().unwrap().code.clone() {
                    return code;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn live_offer_requires_allow_then_pinned_reconnect_and_forget() {
        let hd = Directory::new();
        let vd = Directory::new();
        let host = Arc::new(DeviceIdentity::open(&hd.0).unwrap());
        let viewer = Arc::new(DeviceIdentity::open(&vd.0).unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service = Arc::new(
            SharePairing::new(host.clone(), hd.0.clone(), listener.local_addr().unwrap()).unwrap(),
        );
        let offer = service
            .window
            .lock()
            .unwrap()
            .mode
            .create_offer(&host, listener.local_addr().unwrap())
            .unwrap();
        let (client, server) = connection(&listener, true).await;
        let host_task = {
            let service = service.clone();
            tokio::spawn(async move { service.pair(server).await })
        };
        let (code_tx, code_rx) = oneshot::channel();
        let viewer_task = {
            let viewer = viewer.clone();
            tokio::spawn(async move {
                asynchronous::pair_client_offer(client, &viewer, &offer, |code| async {
                    let _ = code_tx.send(code);
                    true
                })
                .await
            })
        };
        let code = wait_code(&service).await;
        assert_eq!(code_rx.await.unwrap(), code);
        assert!(!host_task.is_finished());
        assert!(DeviceStore::open(&hd.0).unwrap().list().is_empty());
        service.command(Command::Confirm).unwrap();
        host_task.await.unwrap().unwrap();
        let peer = viewer_task.await.unwrap().unwrap();
        DeviceStore::open(&vd.0)
            .unwrap()
            .insert(peer.clone())
            .unwrap();
        assert_eq!(DeviceStore::open(&hd.0).unwrap().list().len(), 1);
        let (client, server) = connection(&listener, false).await;
        let (accepted, connected) = tokio::join!(
            asynchronous::accept_authenticated(server, &host, &hd.0),
            asynchronous::connect(client, &viewer, &peer)
        );
        assert!(accepted.is_ok());
        assert!(connected.is_ok());
        service.command(Command::Forget(viewer.id())).unwrap();
        assert!(DeviceStore::open(&hd.0).unwrap().list().is_empty());
        let (client, server) = connection(&listener, false).await;
        let (accepted, connected) = tokio::join!(
            asynchronous::accept_authenticated(server, &host, &hd.0),
            asynchronous::connect(client, &viewer, &peer)
        );
        assert!(accepted.is_err());
        assert!(connected.is_err());
    }
    #[tokio::test]
    async fn malformed_attempts_lock_valid_offer_and_renew_invalidates_old_offer() {
        let hd = Directory::new();
        let vd = Directory::new();
        let host = Arc::new(DeviceIdentity::open(&hd.0).unwrap());
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            SharePairing::new(host.clone(), hd.0.clone(), listener.local_addr().unwrap()).unwrap();
        let old = service
            .window
            .lock()
            .unwrap()
            .mode
            .create_offer(&host, listener.local_addr().unwrap())
            .unwrap();
        // Certificate/preface failures go through the same live host handshake.
        for _ in 0..3 {
            let (client, server) = connection(&listener, true).await;
            drop(client);
            service.pair(server).await.unwrap();
        }
        assert!(service.window.lock().unwrap().mode.locked());
        let (client, server) = connection(&listener, true).await;
        let (_, result) = tokio::join!(
            service.pair(server),
            asynchronous::pair_client_offer(client, &viewer, &old, |_| async { true })
        );
        assert!(result.is_err());
        service.command(Command::Renew).unwrap();
        let window = service.window.lock().unwrap();
        assert!(!window.mode.locked());
        assert!(window.mode.consume_offer(&old).is_err());
        assert!(DeviceStore::open(&hd.0).unwrap().list().is_empty());
    }
    #[test]
    fn live_metadata_excludes_credentials_and_offer() {
        let hd = Directory::new();
        let host = Arc::new(DeviceIdentity::open(&hd.0).unwrap());
        let service = SharePairing::new(
            host.clone(),
            hd.0.clone(),
            "127.0.0.1:5100".parse().unwrap(),
        )
        .unwrap();
        let metadata = service.metadata().unwrap().records();
        assert_eq!(
            metadata.keys().map(String::as_str).collect::<Vec<_>>(),
            ["id", "name", "wire"]
        );
        let share = trinix_security::ShareIdentity::generate().unwrap();
        let invitation = share
            .invitation("127.0.0.1:5101".parse().unwrap())
            .unwrap()
            .to_link();
        let offer = service
            .window
            .lock()
            .unwrap()
            .mode
            .create_offer(&host, service.address)
            .unwrap()
            .to_offer_string();
        let fingerprint = host
            .fingerprint()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let txt = format!("{metadata:?}");
        for excluded in [&invitation, &offer, &fingerprint] {
            assert!(!txt.contains(excluded));
        }
    }
    #[test]
    fn offer_clock_expiry() {
        struct Future;
        impl Clock for Future {
            fn now(&self) -> u64 {
                SystemClock.now() + 120
            }
        }
        let hd = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let mode = PairingMode::enable();
        let offer = mode
            .create_offer(&host, "127.0.0.1:5100".parse().unwrap())
            .unwrap();
        assert!(PairingOffer::parse(&offer.to_offer_string(), &Future).is_err());
    }
}
