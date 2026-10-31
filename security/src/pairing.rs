//! Explicit mutual-TLS pairing. Discovery is a route hint, never a trust anchor.
use super::*;
pub use crate::pairing_hooks::{
    sanitize_name, Clock, PairingAttemptLimiter, PairingCode, PairingOffer, SystemClock,
};
#[cfg(feature = "async")]
use crate::store::Peer;
use crate::store::{atomic_create, read_private};
use std::path::Path;
const PAIR_ALPN: &[u8] = b"trinix/pair-v2";
#[cfg(feature = "async")]
const PAIRED_ALPN: &[u8] = b"trinix/paired-view-v1";
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
    #[cfg_attr(not(feature = "async"), allow(dead_code))]
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
    const MAGIC_DEVICE: &[u8; 8] = b"TXDEV2\r\n";
    const COMMIT_LABEL: &[u8] = b"Trinix pairing commitment v2";
    const SAS_LABEL: &[u8] = b"Trinix pairing SAS v2";
    const EXPORT_LABEL: &[u8] = b"Trinix pairing SAS v2";
    const NONCE_LEN: usize = 32;
    /// Roles are bound into each commitment so one side's commitment cannot be
    /// reflected back as the other's.
    #[derive(Clone, Copy)]
    pub(super) enum Role {
        Viewer = 1,
        Host = 2,
    }
    pub(super) fn new_nonce() -> io::Result<Zeroizing<[u8; NONCE_LEN]>> {
        let mut nonce = Zeroizing::new([0; NONCE_LEN]);
        getrandom::getrandom(nonce.as_mut()).map_err(tls_error)?;
        Ok(nonce)
    }
    pub(super) fn commitment(role: Role, nonce: &[u8; NONCE_LEN]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(COMMIT_LABEL);
        hash.update([role as u8]);
        hash.update(nonce);
        hash.finalize().into()
    }
    fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
        let mut block = [0u8; 64];
        if key.len() > 64 {
            block[..32].copy_from_slice(&Sha256::digest(key));
        } else {
            block[..key.len()].copy_from_slice(key);
        }
        let mut inner = Sha256::new();
        inner.update(block.map(|b| b ^ 0x36));
        for part in parts {
            inner.update(part);
        }
        let mut outer = Sha256::new();
        outer.update(block.map(|b| b ^ 0x5c));
        outer.update(inner.finalize());
        outer.finalize().into()
    }
    /// First HKDF-SHA256 output block (RFC 5869 extract, then expand with counter 1).
    pub(super) fn hkdf_first_block(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; 32] {
        let prk = hmac_sha256(salt, &[ikm]);
        hmac_sha256(&prk, &[info, &[1]])
    }
    /// The code binds the TLS exporter to BOTH nonces. Each nonce was committed
    /// before the handshake and revealed inside it, so neither can be chosen after
    /// the other party's handshake values are known.
    pub(super) fn sas(
        exporter: [u8; 32],
        viewer_nonce: &[u8; NONCE_LEN],
        host_nonce: &[u8; NONCE_LEN],
    ) -> String {
        let mut salt = [0u8; 2 * NONCE_LEN];
        salt[..NONCE_LEN].copy_from_slice(viewer_nonce);
        salt[NONCE_LEN..].copy_from_slice(host_nonce);
        let okm = hkdf_first_block(&salt, &exporter, SAS_LABEL);
        let n = u32::from_be_bytes([okm[0], okm[1], okm[2], okm[3]]) % 1_000_000;
        format!("{n:06}")
    }
    /// Each side writes its nonce, then reads and verifies the peer's against the
    /// commitment received before TLS. A mismatch is terminal and prompts nobody.
    pub(super) async fn reveal<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        tls: &mut S,
        own: &[u8; NONCE_LEN],
        peer_role: Role,
        peer_commitment: &[u8; 32],
    ) -> io::Result<[u8; NONCE_LEN]> {
        tls.write_all(own).await?;
        tls.flush().await?;
        let mut peer = [0; NONCE_LEN];
        tls.read_exact(&mut peer).await?;
        if !bool::from(commitment(peer_role, &peer).ct_eq(peer_commitment)) {
            return Err(denied());
        }
        Ok(peer)
    }
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
        own_commitment: Option<&[u8; 32]>,
    ) -> io::Result<(Client, [u8; 32], Option<[u8; 32]>)> {
        let pair = own_commitment.is_some();
        let mut magic = [0; 8];
        stream.read_exact(&mut magic).await?;
        if &magic != MAGIC_DEVICE {
            return Err(invalid("Invalid device endpoint"));
        }
        stream.write_all(&[u8::from(pair)]).await?;
        if let Some(commitment) = own_commitment {
            stream.write_all(commitment).await?;
        }
        let cert = read_cert(&mut stream).await?;
        let mut peer_commitment = None;
        if pair {
            let mut value = [0; 32];
            stream.read_exact(&mut value).await?;
            peer_commitment = Some(value);
        }
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
        Ok((tls, fingerprint, peer_commitment))
    }
    pub(super) async fn server(
        mut stream: TcpStream,
        identity: &DeviceIdentity,
        own_commitment: Option<&[u8; 32]>,
    ) -> io::Result<(Server, [u8; 32], Option<[u8; 32]>)> {
        let pair = own_commitment.is_some();
        stream.write_all(MAGIC_DEVICE).await?;
        stream.flush().await?;
        let request = stream.read_u8().await?;
        if request != u8::from(pair) {
            return Err(denied());
        }
        let mut peer_commitment = None;
        if pair {
            let mut value = [0; 32];
            stream.read_exact(&mut value).await?;
            peer_commitment = Some(value);
        }
        write_cert(&mut stream, identity.certificate.as_ref()).await?;
        if let Some(commitment) = own_commitment {
            stream.write_all(commitment).await?;
        }
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
        Ok((tls, fingerprint, peer_commitment))
    }
    pub(super) async fn exchange_name<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
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
            let nonce = new_nonce()?;
            let own = commitment(Role::Viewer, &nonce);
            let (mut tls, fingerprint, host_commitment) =
                client(stream, identity, offer.map(|o| o.fingerprint()), Some(&own)).await?;
            let host_commitment = host_commitment.ok_or_else(denied)?;
            tls.write_u8(u8::from(offer.is_some())).await?;
            if let Some(offer) = offer {
                tls.write_all(&offer.nonce).await?;
            }
            tls.flush().await?;
            let name = exchange_name(&mut tls, identity.friendly_name()).await?;
            let host_nonce = reveal(&mut tls, &nonce, Role::Host, &host_commitment).await?;
            let exporter = tls
                .get_ref()
                .1
                .export_keying_material([0; 32], EXPORT_LABEL, None)
                .map_err(tls_error)?;
            let code = sas(exporter, &nonce, &host_nonce);
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
            let nonce = new_nonce()?;
            let own = commitment(Role::Host, &nonce);
            let (mut tls, fingerprint, viewer_commitment) =
                server(stream, identity, Some(&own)).await?;
            let viewer_commitment = viewer_commitment.ok_or_else(denied)?;
            match tls.read_u8().await? {
                0 => {}
                1 => {
                    let mut offer_nonce = [0; 16];
                    tls.read_exact(&mut offer_nonce).await?;
                    mode.offers.consume(&offer_nonce)?;
                }
                _ => return Err(denied()),
            }
            let name = exchange_name(&mut tls, identity.friendly_name()).await?;
            let viewer_nonce = reveal(&mut tls, &nonce, Role::Viewer, &viewer_commitment).await?;
            let exporter = tls
                .get_ref()
                .1
                .export_keying_material([0; 32], EXPORT_LABEL, None)
                .map_err(tls_error)?;
            let code = sas(exporter, &viewer_nonce, &nonce);
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
            let (mut tls, _, _) = client(stream, identity, Some(peer.fingerprint), None).await?;
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
            let (mut tls, fingerprint, _) = server(stream, identity, None).await?;
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
    use std::sync::atomic::Ordering;
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
            Err(trinix_core::TransportError::Authentication(_))
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
            Err(trinix_core::TransportError::Authentication(_))
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
    #[test]
    fn hkdf_matches_rfc5869_case_1_first_block() {
        let ikm = [0x0b; 22];
        let salt: Vec<u8> = (0..=0x0c).collect();
        let info: Vec<u8> = (0xf0..=0xf9).collect();
        let okm = hkdf_first_block(&salt, &ikm, &info);
        let expected = "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf";
        let hex: String = okm.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, expected);
    }
    #[test]
    fn sas_binds_both_nonces_and_the_exporter() {
        let (exporter, viewer, host) = ([1; 32], [2; 32], [3; 32]);
        let base = sas(exporter, &viewer, &host);
        assert_eq!(base.len(), 6);
        assert_eq!(base, sas(exporter, &viewer, &host));
        // With the same exporter, changing either nonce changes the code.
        // (Single-bit changes; a collision has probability 1e-6 per case.)
        let mut differing = 0;
        for bit in 0..8 {
            let mut v = viewer;
            v[0] ^= 1 << bit;
            let mut h = host;
            h[31] ^= 1 << bit;
            differing += usize::from(sas(exporter, &v, &host) != base);
            differing += usize::from(sas(exporter, &viewer, &h) != base);
        }
        assert!(differing >= 15);
        assert_ne!(sas([9; 32], &viewer, &host), base);
        // The roles are not interchangeable.
        assert_ne!(sas(exporter, &host, &viewer), base);
        // A commitment binds a role and a nonce.
        let c = commitment(Role::Viewer, &viewer);
        assert_ne!(c, commitment(Role::Host, &viewer));
        assert_ne!(c, commitment(Role::Viewer, &host));
    }
    #[tokio::test]
    async fn viewer_nonce_not_matching_its_commitment_is_rejected_before_any_prompt() {
        let hd = Directory::new();
        let ad = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let attacker = DeviceIdentity::open(&ad.0).unwrap();
        let mode = PairingMode::enable();
        let (client_socket, server_socket) = sockets().await;
        let prompted = std::sync::atomic::AtomicBool::new(false);
        let forged = async {
            let committed = new_nonce().unwrap();
            let revealed = new_nonce().unwrap();
            let own = commitment(Role::Viewer, &committed);
            let (mut tls, _, host_commitment) = client(client_socket, &attacker, None, Some(&own))
                .await
                .unwrap();
            let host_commitment = host_commitment.unwrap();
            tls.write_u8(0).await.unwrap();
            exchange_name(&mut tls, "attacker").await.unwrap();
            // Reveal a nonce that differs from the one committed before TLS.
            let result = reveal(&mut tls, &revealed, Role::Host, &host_commitment).await;
            // The host never answers with a decision: it rejects at the reveal.
            let mut byte = [0; 1];
            let read = tls.read(&mut byte).await;
            (result.is_ok(), matches!(read, Ok(0) | Err(_)))
        };
        let (host_result, (forged_accepted_host_nonce, closed)) = tokio::join!(
            pair_host(server_socket, &host, Some(&mode), |_| async {
                prompted.store(true, Ordering::SeqCst);
                true
            }),
            forged
        );
        assert_eq!(
            host_result.err().map(|e| e.kind()),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert!(
            forged_accepted_host_nonce,
            "the genuine host reveal verifies"
        );
        assert!(closed);
        assert!(!prompted.load(Ordering::SeqCst));
        assert!(DeviceStore::open(&hd.0).unwrap().peers().is_empty());
    }
    #[tokio::test]
    async fn host_nonce_not_matching_its_commitment_is_rejected_before_any_prompt() {
        let vd = Directory::new();
        let ad = Directory::new();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let attacker = DeviceIdentity::open(&ad.0).unwrap();
        let (client_socket, server_socket) = sockets().await;
        let prompted = std::sync::atomic::AtomicBool::new(false);
        let forged_host = async {
            let committed = new_nonce().unwrap();
            let revealed = new_nonce().unwrap();
            let own = commitment(Role::Host, &committed);
            let (mut tls, _, viewer_commitment) =
                server(server_socket, &attacker, Some(&own)).await.unwrap();
            let viewer_commitment = viewer_commitment.unwrap();
            assert_eq!(tls.read_u8().await.unwrap(), 0);
            exchange_name(&mut tls, "attacker").await.unwrap();
            let verified = reveal(&mut tls, &revealed, Role::Viewer, &viewer_commitment).await;
            verified.is_ok()
        };
        let (client_result, genuine_viewer_verified) = tokio::join!(
            pair_client(client_socket, &viewer, |_| async {
                prompted.store(true, Ordering::SeqCst);
                true
            }),
            forged_host
        );
        assert_eq!(
            client_result.err().map(|e| e.kind()),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert!(genuine_viewer_verified);
        assert!(!prompted.load(Ordering::SeqCst));
        assert!(DeviceStore::open(&vd.0).unwrap().peers().is_empty());
    }
    #[tokio::test]
    async fn mitm_terminating_two_pairing_legs_shows_different_codes() {
        let hd = Directory::new();
        let vd = Directory::new();
        let ad = Directory::new();
        let host = DeviceIdentity::open(&hd.0).unwrap();
        let viewer = DeviceIdentity::open(&vd.0).unwrap();
        let attacker = DeviceIdentity::open(&ad.0).unwrap();
        let host_mode = PairingMode::enable();
        let attacker_mode = PairingMode::enable();
        let (attack_to_host, host_socket) = sockets().await;
        let (viewer_socket, attack_to_viewer) = sockets().await;
        let codes = std::sync::Mutex::new(std::collections::HashMap::new());
        let note = |who: &'static str, code: String| {
            codes.lock().unwrap().insert(who, code);
        };
        // The intermediary runs the complete protocol on both legs with its own
        // commitments and nonces; it cannot reuse either party's.
        let (h, ah, av, v) = tokio::join!(
            pair_host(host_socket, &host, Some(&host_mode), |code| {
                note("host", code);
                async { true }
            }),
            pair_client(attack_to_host, &attacker, |code| {
                note("attacker-to-host", code);
                async { true }
            }),
            pair_host(attack_to_viewer, &attacker, Some(&attacker_mode), |code| {
                note("attacker-to-viewer", code);
                async { true }
            }),
            pair_client(viewer_socket, &viewer, |code| {
                note("viewer", code);
                async { true }
            })
        );
        let codes = codes.into_inner().unwrap();
        // Each leg is internally consistent...
        assert_eq!(codes["host"], codes["attacker-to-host"]);
        assert_eq!(codes["viewer"], codes["attacker-to-viewer"]);
        // ...but the two honest parties are shown different codes, so a user who
        // compares them rejects the pairing.
        assert_ne!(codes["host"], codes["viewer"]);
        // Each honest party paired with the intermediary's key, not the other's.
        let h = h.unwrap();
        let v = v.unwrap();
        assert_eq!(h.fingerprint, attacker.fingerprint());
        assert_eq!(v.fingerprint, attacker.fingerprint());
        assert_ne!(h.fingerprint, viewer.fingerprint());
        assert_ne!(v.fingerprint, host.fingerprint());
        drop((ah, av));
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
