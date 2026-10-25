//! Explicit mutual-TLS pairing. Discovery is a route hint, never a trust anchor.
use super::*;
pub use crate::pairing_hooks::{
    sanitize_name, Clock, PairingAttemptLimiter, PairingCode, PairingOffer, SystemClock,
};
#[cfg(feature = "async")]
use crate::store::Peer;
use crate::store::{atomic_create, read_private};
use std::path::Path;
const PAIR_ALPN: &[u8] = b"mightymonitor/pair-v1";
#[cfg(feature = "async")]
const PAIRED_ALPN: &[u8] = b"mightymonitor/paired-view-v1";
pub const PAIRING_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
pub struct DeviceIdentity {
    directory: std::path::PathBuf,
    name: String,
    certificate: CertificateDer<'static>,
    key: Zeroizing<Vec<u8>>,
    fingerprint: [u8; 32],
}
impl fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceIdentity { credentials: [redacted] }")
    }
}
impl DeviceIdentity {
    pub fn open(directory: &Path) -> io::Result<Self> {
        let name_path = directory.join("name.bin");
        let name = match read_private(&name_path) {
            Ok(bytes) => {
                let name =
                    std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid device name"))?;
                if name.len() > 128 || sanitize_name(name) != name {
                    return Err(invalid("Invalid device name"));
                }
                name.to_string()
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let name = crate::pairing_hooks::default_name();
                atomic_create(&name_path, name.as_bytes())?;
                name
            }
            Err(e) => return Err(e),
        };
        let path = directory.join("identity.bin");
        match read_private(&path) {
            Ok(bytes) => {
                if bytes.len() < 12 || &bytes[..8] != b"MMIDENT1" {
                    return Err(invalid("Invalid device identity"));
                }
                let length =
                    u32::from_le_bytes(bytes[8..12].try_into().map_err(tls_error)?) as usize;
                if length == 0 || length > CERT_LIMIT || bytes.len() <= 12 + length {
                    return Err(invalid("Invalid device identity"));
                }
                let certificate = CertificateDer::from(bytes[12..12 + length].to_vec());
                let fingerprint = Sha256::digest(certificate.as_ref()).into();
                let identity = Self {
                    directory: directory.to_path_buf(),
                    name,
                    certificate,
                    key: Zeroizing::new(bytes[12 + length..].to_vec()),
                    fingerprint,
                };
                identity.server_config(None, PAIR_ALPN)?;
                Ok(identity)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let rcgen::CertifiedKey { cert, signing_key } =
                    rcgen::generate_simple_self_signed(vec![SERVER_NAME.into()])
                        .map_err(tls_error)?;
                let certificate = cert.der().clone();
                let fingerprint = Sha256::digest(certificate.as_ref()).into();
                let key = Zeroizing::new(signing_key.serialize_der());
                let mut bytes = Zeroizing::new(b"MMIDENT1".to_vec());
                bytes.extend_from_slice(&(certificate.len() as u32).to_le_bytes());
                bytes.extend_from_slice(certificate.as_ref());
                bytes.extend_from_slice(&key);
                atomic_create(&path, &bytes)?;
                Ok(Self {
                    directory: directory.to_path_buf(),
                    name,
                    certificate,
                    key,
                    fingerprint,
                })
            }
            Err(e) => Err(e),
        }
    }
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn friendly_name(&self) -> &str {
        &self.name
    }
    pub fn set_friendly_name(&mut self, value: &str) -> io::Result<()> {
        let name = sanitize_name(value);
        crate::store::atomic_write(&self.directory.join("name.bin"), name.as_bytes())?;
        self.name = name;
        Ok(())
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }
    pub fn id(&self) -> String {
        crate::store::short_id(&self.fingerprint)
    }
    fn server_config(&self, peer: Option<Vec<u8>>, alpn: &[u8]) -> io::Result<Arc<ServerConfig>> {
        let builder = server_builder()?;
        let builder = if let Some(cert) = peer {
            let mut roots = RootCertStore::empty();
            roots.add(CertificateDer::from(cert)).map_err(tls_error)?;
            builder.with_client_cert_verifier(
                rustls::server::WebPkiClientVerifier::builder_with_provider(
                    Arc::new(roots),
                    Arc::new(rustls::crypto::ring::default_provider()),
                )
                .build()
                .map_err(tls_error)?,
            )
        } else {
            builder.with_no_client_auth()
        };
        let mut config = builder
            .with_single_cert(
                vec![self.certificate.clone()],
                PrivatePkcs8KeyDer::from(self.key.to_vec()).into(),
            )
            .map_err(tls_error)?;
        config.alpn_protocols = vec![alpn.to_vec()];
        config.max_early_data_size = 0;
        Ok(Arc::new(config))
    }
    #[cfg(feature = "async")]
    fn client_config(&self, cert: Vec<u8>, alpn: &[u8]) -> io::Result<Arc<ClientConfig>> {
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from(cert)).map_err(tls_error)?;
        let mut config = client_builder()?
            .with_root_certificates(roots)
            .with_client_auth_cert(
                vec![self.certificate.clone()],
                PrivatePkcs8KeyDer::from(self.key.to_vec()).into(),
            )
            .map_err(tls_error)?;
        config.alpn_protocols = vec![alpn.to_vec()];
        config.enable_early_data = false;
        Ok(Arc::new(config))
    }
}
/// One active attempt per explicit, expiring local pairing window. Attempts are
/// spaced by two seconds, including failures. Dropping a permit releases capacity.
pub struct PairingMode {
    offers: crate::pairing_hooks::Offers,
    end: std::time::Instant,
    busy: std::sync::atomic::AtomicBool,
    last: std::sync::Mutex<Option<std::time::Instant>>,
}
pub struct PairingPermit<'a>(&'a PairingMode);
impl Drop for PairingPermit<'_> {
    fn drop(&mut self) {
        self.0
            .busy
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
impl PairingMode {
    pub fn enable() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            offers: crate::pairing_hooks::Offers::new(clock),
            end: std::time::Instant::now() + PAIRING_WINDOW,
            busy: std::sync::atomic::AtomicBool::new(false),
            last: std::sync::Mutex::new(None),
        }
    }
    pub fn create_offer(
        &self,
        identity: &DeviceIdentity,
        address: SocketAddr,
    ) -> io::Result<PairingOffer> {
        self.offers.create(identity.fingerprint(), address)
    }
    /// Consume before attempting authorization; failures must not restore it.
    pub fn consume_offer(&self, offer: &PairingOffer) -> io::Result<()> {
        self.offers.consume(&offer.nonce)
    }
    pub fn enter(&self) -> io::Result<PairingPermit<'_>> {
        let now = std::time::Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        if now >= self.end
            || last.is_some_and(|t| now.duration_since(t) < std::time::Duration::from_secs(2))
            || self.busy.swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(denied());
        }
        *last = Some(now);
        Ok(PairingPermit(self))
    }
}
#[cfg(feature = "async")]
pub mod asynchronous {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
    };
    use tokio_rustls::{TlsAcceptor, TlsConnector};
    type Client = tokio_rustls::client::TlsStream<TcpStream>;
    type Server = tokio_rustls::server::TlsStream<TcpStream>;
    async fn write_cert(stream: &mut TcpStream, cert: &[u8]) -> io::Result<()> {
        stream.write_all(&(cert.len() as u16).to_le_bytes()).await?;
        stream.write_all(cert).await?;
        stream.flush().await
    }
    async fn read_cert(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
        let length = stream.read_u16_le().await? as usize;
        if length == 0 || length > CERT_LIMIT {
            return Err(invalid("Invalid device certificate"));
        }
        let mut cert = vec![0; length];
        stream.read_exact(&mut cert).await?;
        Ok(cert)
    }
    pub(super) async fn client(
        mut stream: TcpStream,
        identity: &DeviceIdentity,
        pin: Option<[u8; 32]>,
        pair: bool,
    ) -> io::Result<(Client, [u8; 32])> {
        let mut magic = [0; 8];
        stream.read_exact(&mut magic).await?;
        if &magic != b"MMDEV1\r\n" {
            return Err(invalid("Invalid device endpoint"));
        }
        stream.write_all(&[u8::from(pair)]).await?;
        let cert = read_cert(&mut stream).await?;
        let fingerprint: [u8; 32] = Sha256::digest(&cert).into();
        if pin.is_some_and(|p| !bool::from(p.ct_eq(&fingerprint))) {
            return Err(denied());
        }
        write_cert(&mut stream, identity.certificate.as_ref()).await?;
        let alpn = if pair { PAIR_ALPN } else { PAIRED_ALPN };
        let tls = TlsConnector::from(identity.client_config(cert, alpn)?)
            .connect(
                rustls::pki_types::ServerName::try_from(SERVER_NAME).map_err(tls_error)?,
                stream,
            )
            .await?;
        if tls.get_ref().1.alpn_protocol() != Some(alpn) {
            return Err(denied());
        }
        let leaf = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|chain| chain.first())
            .ok_or_else(denied)?;
        if !bool::from(Sha256::digest(leaf.as_ref()).as_slice().ct_eq(&fingerprint)) {
            return Err(denied());
        }
        Ok((tls, fingerprint))
    }
    pub(super) async fn server(
        mut stream: TcpStream,
        identity: &DeviceIdentity,
        pair: bool,
    ) -> io::Result<(Server, [u8; 32])> {
        stream.write_all(b"MMDEV1\r\n").await?;
        stream.flush().await?;
        let request = stream.read_u8().await?;
        if request != u8::from(pair) {
            return Err(denied());
        }
        write_cert(&mut stream, identity.certificate.as_ref()).await?;
        let cert = read_cert(&mut stream).await?;
        let fingerprint: [u8; 32] = Sha256::digest(&cert).into();
        let alpn = if pair { PAIR_ALPN } else { PAIRED_ALPN };
        let tls = TlsAcceptor::from(identity.server_config(Some(cert), alpn)?)
            .accept(stream)
            .await?;
        if tls.get_ref().1.alpn_protocol() != Some(alpn) {
            return Err(denied());
        }
        let leaf = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|chain| chain.first())
            .ok_or_else(denied)?;
        if !bool::from(Sha256::digest(leaf.as_ref()).as_slice().ct_eq(&fingerprint)) {
            return Err(denied());
        }
        Ok((tls, fingerprint))
    }
    async fn exchange_name<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        tls: &mut S,
        name: &str,
    ) -> io::Result<String> {
        tls.write_u8(name.len() as u8).await?;
        tls.write_all(name.as_bytes()).await?;
        tls.flush().await?;
        let len = tls.read_u8().await? as usize;
        if len == 0 || len > 128 {
            return Err(denied());
        }
        let mut bytes = vec![0; len];
        tls.read_exact(&mut bytes).await?;
        let value = std::str::from_utf8(&bytes).map_err(|_| denied())?;
        Ok(sanitize_name(value))
    }
    fn sas(bytes: [u8; 32]) -> String {
        let n = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1_000_000;
        format!("{n:06}")
    }
    /// Confirmation future must represent a fresh local user decision, never a
    /// pre-approved setting. Entire attempt, including user interaction, is bounded.
    pub async fn pair_client<F, Fut>(
        stream: TcpStream,
        identity: &DeviceIdentity,
        confirm: F,
    ) -> io::Result<Peer>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        pair_client_inner(stream, identity, None, confirm).await
    }
    /// QR offer pins TLS before SAS; host confirmation remains mandatory.
    pub async fn pair_client_offer<F, Fut>(
        stream: TcpStream,
        identity: &DeviceIdentity,
        offer: &PairingOffer,
        confirm: F,
    ) -> io::Result<Peer>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        pair_client_inner(stream, identity, Some(offer), confirm).await
    }
    async fn pair_client_inner<F, Fut>(
        stream: TcpStream,
        identity: &DeviceIdentity,
        offer: Option<&PairingOffer>,
        confirm: F,
    ) -> io::Result<Peer>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        tokio::time::timeout(PAIRING_WINDOW, async {
            if offer.is_some_and(|o| o.expires_at() <= SystemClock.now()) {
                return Err(denied());
            }
            let (mut tls, fingerprint) =
                client(stream, identity, offer.map(|o| o.fingerprint()), true).await?;
            tls.write_u8(u8::from(offer.is_some())).await?;
            if let Some(offer) = offer {
                tls.write_all(&offer.nonce).await?;
            }
            tls.flush().await?;
            let name = exchange_name(&mut tls, identity.friendly_name()).await?;
            let code = sas(tls
                .get_ref()
                .1
                .export_keying_material([0; 32], b"MightyMonitor pairing SAS v1", None)
                .map_err(tls_error)?);
            let accepted = confirm(code).await;
            tls.write_all(&[u8::from(accepted)]).await?;
            tls.flush().await?;
            if !accepted || tls.read_u8().await? != 1 {
                return Err(denied());
            }
            let mut secret = Zeroizing::new([0; 32]);
            tls.read_exact(secret.as_mut()).await?;
            tls.write_all(&[1]).await?;
            tls.flush().await?;
            Ok(Peer::new(fingerprint, secret, name))
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pairing timed out"))?
    }
    pub async fn pair_host<F, Fut>(
        stream: TcpStream,
        identity: &DeviceIdentity,
        mode: Option<&PairingMode>,
        confirm: F,
    ) -> io::Result<Peer>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let mode = mode.ok_or_else(denied)?;
        let _permit = mode.enter()?;
        let remaining = mode
            .end
            .saturating_duration_since(std::time::Instant::now());
        tokio::time::timeout(remaining, async {
            let (mut tls, fingerprint) = server(stream, identity, true).await?;
            match tls.read_u8().await? {
                0 => {}
                1 => {
                    let mut nonce = [0; 16];
                    tls.read_exact(&mut nonce).await?;
                    mode.offers.consume(&nonce)?;
                }
                _ => return Err(denied()),
            }
            let name = exchange_name(&mut tls, identity.friendly_name()).await?;
            let code = sas(tls
                .get_ref()
                .1
                .export_keying_material([0; 32], b"MightyMonitor pairing SAS v1", None)
                .map_err(tls_error)?);
            let accepted = confirm(code).await;
            if tls.read_u8().await? != 1 || !accepted {
                let _ = tls.write_all(&[0]).await;
                return Err(denied());
            }
            let mut secret = Zeroizing::new([0; 32]);
            getrandom::getrandom(secret.as_mut()).map_err(tls_error)?;
            tls.write_all(&[1]).await?;
            tls.write_all(secret.as_slice()).await?;
            tls.flush().await?;
            if tls.read_u8().await? != 1 {
                return Err(denied());
            }
            Ok(Peer::new(fingerprint, secret, name))
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pairing timed out"))?
    }
    pub async fn connect(
        stream: TcpStream,
        identity: &DeviceIdentity,
        peer: &Peer,
    ) -> io::Result<Client> {
        tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            let (mut tls, _) = client(stream, identity, Some(peer.fingerprint), false).await?;
            tls.write_all(peer.secret.as_slice()).await?;
            tls.flush().await?;
            if tls.read_u8().await? != 1 {
                return Err(denied());
            }
            Ok(tls)
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Paired connection timed out"))?
    }
    /// Reopens the store for each authorization: revocation affects the next dial.
    /// The returned transport carries view authority only.
    pub async fn accept(
        stream: TcpStream,
        identity: &DeviceIdentity,
        directory: &Path,
    ) -> io::Result<Server> {
        Ok(accept_authenticated(stream, identity, directory).await?.0)
    }
    pub async fn accept_authenticated(
        stream: TcpStream,
        identity: &DeviceIdentity,
        directory: &Path,
    ) -> io::Result<(Server, AuthenticatedPeer)> {
        tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            let (mut tls, fingerprint) = server(stream, identity, false).await?;
            let mut secret = Zeroizing::new([0; 32]);
            tls.read_exact(secret.as_mut()).await?;
            let mut store = crate::store::DeviceStore::open(directory)?;
            let accepted = store.peers().iter().any(|p| {
                bool::from(
                    p.fingerprint.ct_eq(&fingerprint)
                        & p.secret.as_slice().ct_eq(secret.as_slice()),
                )
            });
            tls.write_all(&[u8::from(accepted)]).await?;
            tls.flush().await?;
            if !accepted {
                return Err(denied());
            }
            store.seen(&fingerprint)?;
            Ok((
                tls,
                AuthenticatedPeer::paired(fingerprint, secret.as_slice(), directory.to_path_buf()),
            ))
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Paired connection timed out"))?
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use super::asynchronous::*;
    use super::*;
    use crate::store::{tests::Directory, DeviceStore};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    async fn sockets() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (client, server) = tokio::join!(
            TcpStream::connect(listener.local_addr().unwrap()),
            listener.accept()
        );
        (client.unwrap(), server.unwrap().0)
    }
    async fn attempt(
        host: &DeviceIdentity,
        viewer: &DeviceIdentity,
        hyes: bool,
        vyes: bool,
    ) -> (io::Result<Peer>, io::Result<Peer>, Vec<String>) {
        let (client, server) = sockets().await;
        let mode = PairingMode::enable();
        let codes = std::sync::Mutex::new(Vec::new());
        let (h, v) = tokio::join!(
            pair_host(server, host, Some(&mode), |code| {
                codes.lock().unwrap().push(code);
                async move { hyes }
            }),
            pair_client(client, viewer, |code| {
                codes.lock().unwrap().push(code);
                async move { vyes }
            })
        );
        (h, v, codes.into_inner().unwrap())
    }
    #[tokio::test]
    async fn matching_sas_both_confirm_persistence_reconnect_and_revocation() {
        let hd = Directory::new();
        let vd = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let (h, v, codes) = attempt(&host, &viewer, true, true).await;
        let h = h.unwrap();
        let v = v.unwrap();
        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0], codes[1]);
        assert_eq!(codes[0].len(), 6);
        assert_eq!(h.fingerprint, viewer.fingerprint());
        assert_eq!(v.fingerprint, host.fingerprint());
        assert_eq!(*h.secret, *v.secret);
        DeviceStore::open(&hd.0).unwrap().insert(h.clone()).unwrap();
        DeviceStore::open(&vd.0).unwrap().insert(v.clone()).unwrap();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let (client, server) = sockets().await;
        let (hresult, vresult) =
            tokio::join!(accept(server, &host, &hd.0), connect(client, &viewer, &v));
        let mut hchannel = hresult.unwrap();
        let mut vchannel = vresult.unwrap();
        vchannel.write_all(b"view").await.unwrap();
        vchannel.flush().await.unwrap();
        let mut bytes = [0; 4];
        hchannel.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"view");
        let mut wrong = v.clone();
        wrong.secret[0] ^= 1;
        let (client, server) = sockets().await;
        let (a, b) = tokio::join!(
            accept(server, &host, &hd.0),
            connect(client, &viewer, &wrong)
        );
        assert!(a.is_err() && b.is_err());
        let mut wrong = v.clone();
        wrong.fingerprint[0] ^= 1;
        let (client, server) = sockets().await;
        let (a, b) = tokio::join!(
            accept(server, &host, &hd.0),
            connect(client, &viewer, &wrong)
        );
        assert!(a.is_err() && b.is_err());
        DeviceStore::open(&hd.0).unwrap().unpair(&h.id()).unwrap();
        let (client, server) = sockets().await;
        let (a, b) = tokio::join!(accept(server, &host, &hd.0), connect(client, &viewer, &v));
        assert!(a.is_err() && b.is_err());
    }
    #[tokio::test]
    async fn unpaired_rejected_and_offer_pinning_still_requires_confirmation() {
        let hd = Directory::new();
        let vd = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let (h, v, _) = attempt(&host, &viewer, true, true).await;
        let h = h.unwrap();
        let v = v.unwrap();
        // Neither returned credential has been persisted at the host.
        let (client, server) = sockets().await;
        let (a, b) = tokio::join!(accept(server, &host, &hd.0), connect(client, &viewer, &v));
        assert!(a.is_err() && b.is_err());
        let mode = PairingMode::enable();
        let offer = mode
            .create_offer(&host, "127.0.0.1:5000".parse().unwrap())
            .unwrap();
        let (client, server) = sockets().await;
        let (a, b) = tokio::join!(
            pair_host(server, &host, Some(&mode), |_| async { false }),
            pair_client_offer(client, &viewer, &offer, |_| async { true })
        );
        assert!(a.is_err() && b.is_err());
        assert!(mode.consume_offer(&offer).is_err());
        assert!(DeviceStore::open(&hd.0).unwrap().list().is_empty());
        // The transport adapter makes a wrong pin terminal, even with legacy enabled elsewhere.
        DeviceStore::open(&vd.0).unwrap().insert(v.clone()).unwrap();
        DeviceStore::open(&hd.0).unwrap().insert(h).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut wrong = v;
        wrong.fingerprint[0] ^= 1;
        // Persist the deliberately wrong pin as a credential to exercise TLS pin rejection.
        DeviceStore::open(&vd.0)
            .unwrap()
            .insert(wrong.clone())
            .unwrap();
        let endpoint = Endpoint::Paired {
            address: listener.local_addr().unwrap(),
            identity: Arc::new(viewer),
            peer: wrong,
        };
        let (a, b) = tokio::join!(
            async {
                let (s, _) = listener.accept().await.unwrap();
                accept(s, &host, &hd.0).await
            },
            crate::asynchronous::connect_transport(&endpoint)
        );
        assert!(a.is_err());
        assert!(matches!(
            b,
            Err(mightymonitor_core::TransportError::Authentication(_))
        ));
    }
    #[tokio::test]
    async fn spoofed_advertisement_cannot_replace_stored_pin() {
        let hd = Directory::new();
        let vd = Directory::new();
        let ad = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let mut attacker = DeviceIdentity::open(&ad.0).unwrap();
        attacker.set_friendly_name(host.friendly_name()).unwrap();
        let (_, peer, _) = attempt(&host, &viewer, true, true).await;
        let peer = peer.unwrap();
        DeviceStore::open(&vd.0)
            .unwrap()
            .insert(peer.clone())
            .unwrap();
        // Discovery can claim any ID/name. Only its route is used here;
        // the expected pin comes from the locally selected store entry.
        let claimed_id = peer.id();
        let selected = DeviceStore::open(&vd.0).unwrap().find(&claimed_id).unwrap();
        assert_eq!(attacker.friendly_name(), host.friendly_name());
        assert_ne!(attacker.fingerprint(), selected.fingerprint);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint::Paired {
            address: listener.local_addr().unwrap(),
            identity: Arc::new(viewer),
            peer: selected,
        };
        let (server, client) = tokio::join!(
            async {
                let (stream, _) = listener.accept().await.unwrap();
                accept(stream, &attacker, &ad.0).await
            },
            crate::asynchronous::connect_transport(&endpoint)
        );
        assert!(server.is_err());
        assert!(matches!(
            client,
            Err(mightymonitor_core::TransportError::Authentication(_))
        ));
    }

    #[tokio::test]
    async fn colliding_display_ids_do_not_affect_authentication() {
        let hd = Directory::new();
        let vd = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let (hp, vp, _) = attempt(&host, &viewer, true, true).await;
        DeviceStore::open(&hd.0)
            .unwrap()
            .insert(hp.unwrap())
            .unwrap();
        let peer = vp.unwrap();
        let mut collision = peer.clone();
        collision.fingerprint[31] ^= 1;
        let mut store = DeviceStore::open(&vd.0).unwrap();
        store.insert(peer.clone()).unwrap();
        store.insert(collision.clone()).unwrap();
        assert_eq!(peer.id(), collision.id());
        assert!(store.find(&peer.id()).is_err());
        assert!(store.forget(&peer.id()).is_err());
        assert!(store.unpair(&peer.id()).is_err());
        assert_eq!(DeviceStore::open(&vd.0).unwrap().peers().len(), 2);
        for pin in [peer.fingerprint, collision.fingerprint] {
            assert_eq!(store.find_by_fingerprint(&pin).unwrap().fingerprint, pin);
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint::Paired {
            address: listener.local_addr().unwrap(),
            identity: Arc::new(viewer),
            peer,
        };
        let (server, client) = tokio::join!(
            async {
                let (stream, _) = listener.accept().await.unwrap();
                accept(stream, &host, &hd.0).await
            },
            crate::asynchronous::connect_transport(&endpoint)
        );
        assert!(server.is_ok());
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn either_denial_leaves_no_peer() {
        let hd = Directory::new();
        let vd = Directory::new();
        let ad = Directory::new();
        let h = DeviceIdentity::open(&hd.0).unwrap();
        let v = DeviceIdentity::open(&vd.0).unwrap();
        let attacker = DeviceIdentity::open(&ad.0).unwrap();
        let (a, b, first) = attempt(&h, &v, false, true).await;
        assert!(a.is_err() && b.is_err());
        let (a, b, second) = attempt(&attacker, &v, true, false).await;
        assert!(a.is_err() && b.is_err());
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        // Full exporter separation is checked independently; six-digit SAS can collide.
        assert!(DeviceStore::open(&hd.0).unwrap().peers().is_empty());
        assert!(DeviceStore::open(&vd.0).unwrap().peers().is_empty());
    }
    #[tokio::test]
    async fn disabled_pairing_and_concurrent_attempt_rejected() {
        let dir = Directory::new();
        let identity = DeviceIdentity::open(&dir.0).unwrap();
        let (_client, server) = sockets().await;
        assert!(pair_host(server, &identity, None, |_| async { true })
            .await
            .is_err());
        let mode = PairingMode::enable();
        let permit = mode.enter().unwrap();
        let (_client, server) = sockets().await;
        assert!(
            pair_host(server, &identity, Some(&mode), |_| async { true })
                .await
                .is_err()
        );
        assert!(mode.enter().is_err());
        drop(permit);
        assert!(mode.enter().is_err());
    }
    #[tokio::test]
    async fn mitm_terminating_two_tls_legs_has_different_exporter_material() {
        let hd = Directory::new();
        let vd = Directory::new();
        let ad = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let attacker = DeviceIdentity::open(&ad.0).unwrap();
        let (attack_to_host, host_socket) = sockets().await;
        let (viewer_socket, attack_to_viewer) = sockets().await;
        let (h, ah, av, v) = tokio::join!(
            super::asynchronous::server(host_socket, &host, true),
            super::asynchronous::client(attack_to_host, &attacker, None, true),
            super::asynchronous::server(attack_to_viewer, &attacker, true),
            super::asynchronous::client(viewer_socket, &viewer, None, true)
        );
        let h = h.unwrap().0;
        let ah = ah.unwrap().0;
        let av = av.unwrap().0;
        let v = v.unwrap().0;
        let label = b"MightyMonitor pairing SAS v1";
        let host_material = h
            .get_ref()
            .1
            .export_keying_material([0; 32], label, None)
            .unwrap();
        let viewer_material = v
            .get_ref()
            .1
            .export_keying_material([0; 32], label, None)
            .unwrap();
        assert_eq!(
            host_material,
            ah.get_ref()
                .1
                .export_keying_material([0; 32], label, None)
                .unwrap()
        );
        assert_eq!(
            viewer_material,
            av.get_ref()
                .1
                .export_keying_material([0; 32], label, None)
                .unwrap()
        );
        assert_ne!(host_material, viewer_material);
        assert!(DeviceStore::open(&hd.0).unwrap().peers().is_empty());
        assert!(DeviceStore::open(&vd.0).unwrap().peers().is_empty());
    }
    #[tokio::test(start_paused = true)]
    async fn stalled_pairing_times_out_without_state() {
        let dir = Directory::new();
        let identity = DeviceIdentity::open(&dir.0).unwrap();
        let mode = PairingMode::enable();
        let (_client, server) = sockets().await;
        assert_eq!(
            pair_host(server, &identity, Some(&mode), |_| async { true })
                .await
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(DeviceStore::open(&dir.0).unwrap().peers().is_empty());
    }
}
