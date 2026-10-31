//! Explicit, revocable local viewing invitations over pinned TLS 1.3.
//!
//! An invitation carries a public certificate fingerprint and a random bearer
//! token. Transfer the complete invitation directly from the sharing device.
//! Discovery/address information alone never grants trust. No certificate
//! verification bypass or plaintext fallback is used on a secure connection.
#![forbid(unsafe_code)]
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    ClientConfig, RootCertStore, ServerConfig,
};
use sha2::{Digest, Sha256};
use std::{fmt, io, net::SocketAddr, str::FromStr, sync::Arc};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
#[cfg(feature = "async")]
pub mod asynchronous;
pub mod blocking;
pub mod device_cli;
pub mod pairing;
mod pairing_hooks;
mod peer;
pub mod store;
pub use peer::{random_session_id, AuthenticatedPeer};
const MAGIC: &[u8; 8] = b"MMTLS1\r\n";
const AUTH: &[u8; 8] = b"MMAUTH1\0";
const ALPN: &[u8] = b"mightymonitor/secure-v1";
const SERVER_NAME: &str = "mightymonitor.local";
const CERT_LIMIT: usize = 4096;
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "The connection link is invalid or has expired",
    )
}
fn tls_error(error: impl fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Secure connection failed: {error}"),
    )
}

fn server_builder() -> io::Result<rustls::ConfigBuilder<ServerConfig, rustls::WantsVerifier>> {
    ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(tls_error)
}
fn client_builder() -> io::Result<rustls::ConfigBuilder<ClientConfig, rustls::WantsVerifier>> {
    ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(tls_error)
}

/// Session-local identity. A new identity rejects every previous invitation.
/// The host must also close established channels when stopping sharing.
/// Neither certificate private keys nor bearer tokens are printed by Debug.
pub struct ShareIdentity {
    certificate: CertificateDer<'static>,
    config: Arc<ServerConfig>,
    fingerprint: [u8; 32],
    token: Zeroizing<[u8; 32]>,
}
impl fmt::Debug for ShareIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShareIdentity").finish_non_exhaustive()
    }
}
impl ShareIdentity {
    pub fn generate() -> io::Result<Self> {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec![SERVER_NAME.into()]).map_err(tls_error)?;
        let certificate = cert.der().clone();
        let fingerprint = Sha256::digest(certificate.as_ref()).into();
        let key = PrivatePkcs8KeyDer::from(signing_key.serialize_der());
        let mut config = server_builder()?
            .with_no_client_auth()
            .with_single_cert(vec![certificate.clone()], key.into())
            .map_err(tls_error)?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.max_early_data_size = 0;
        let mut token = Zeroizing::new([0; 32]);
        getrandom::getrandom(&mut *token).map_err(tls_error)?;
        Ok(Self {
            certificate,
            config: Arc::new(config),
            fingerprint,
            token,
        })
    }
    pub fn invitation(&self, address: SocketAddr) -> io::Result<Invitation> {
        validate_address(address)?;
        Ok(Invitation {
            address,
            fingerprint: self.fingerprint,
            token: Zeroizing::new(*self.token),
        })
    }
    fn authenticate(&self, bytes: &[u8; 40]) -> bool {
        let prefix = bytes[..8].ct_eq(AUTH.as_slice());
        let token = bytes[8..].ct_eq(self.token.as_slice());
        bool::from(prefix & token)
    }
}

/// Possession authorizes screen viewing for one explicitly started host session.
/// Call to_link only for intentional transfer/display; Debug always redacts it.
#[derive(Clone)]
pub struct Invitation {
    address: SocketAddr,
    fingerprint: [u8; 32],
    token: Zeroizing<[u8; 32]>,
}
impl fmt::Debug for Invitation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invitation")
            .field("address", &self.address)
            .field("credentials", &"[redacted]")
            .finish()
    }
}
impl Invitation {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn to_link(&self) -> String {
        format!(
            "mightymonitor://{}/{}/{}",
            self.address,
            URL_SAFE_NO_PAD.encode(self.fingerprint),
            URL_SAFE_NO_PAD.encode(self.token.as_slice())
        )
    }
    /// Keep identity/authentication while changing the network route (e.g. a
    /// consented USB/ADB tunnel). The pinned host still has to prove its identity.
    pub fn via_address(mut self, address: SocketAddr) -> io::Result<Self> {
        validate_address(address)?;
        self.address = address;
        Ok(self)
    }
    fn client_config(&self, cert: Vec<u8>) -> io::Result<Arc<ClientConfig>> {
        if cert.is_empty()
            || cert.len() > CERT_LIMIT
            || !bool::from(
                Sha256::digest(&cert)
                    .as_slice()
                    .ct_eq(self.fingerprint.as_slice()),
            )
        {
            return Err(denied());
        }
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from(cert)).map_err(tls_error)?;
        let mut config = client_builder()?
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.enable_early_data = false;
        Ok(Arc::new(config))
    }
    fn authentication(&self) -> Zeroizing<[u8; 40]> {
        let mut bytes = Zeroizing::new([0; 40]);
        bytes[..8].copy_from_slice(AUTH);
        bytes[8..].copy_from_slice(self.token.as_slice());
        bytes
    }
}
impl FromStr for Invitation {
    type Err = io::Error;
    fn from_str(link: &str) -> io::Result<Self> {
        if link.len() > 512 {
            return Err(invalid("Connection link is too long"));
        }
        let mut parts = link
            .trim()
            .strip_prefix("mightymonitor://")
            .ok_or_else(|| invalid("Paste a Trinix connection link"))?
            .split('/');
        let address: SocketAddr = parts
            .next()
            .ok_or_else(|| invalid("Missing sharing address"))?
            .parse()
            .map_err(|_| invalid("Invalid sharing address"))?;
        validate_address(address)?;
        let fingerprint: [u8; 32] = URL_SAFE_NO_PAD
            .decode(
                parts
                    .next()
                    .ok_or_else(|| invalid("Missing host identity"))?,
            )
            .map_err(|_| invalid("Invalid host identity"))?
            .try_into()
            .map_err(|_| invalid("Invalid host identity"))?;
        let secret = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(
                    parts
                        .next()
                        .ok_or_else(|| invalid("Missing viewing permission"))?,
                )
                .map_err(|_| invalid("Invalid viewing permission"))?,
        );
        if secret.len() != 32 || parts.next().is_some() {
            return Err(invalid("Invalid connection link"));
        }
        let mut token = Zeroizing::new([0; 32]);
        token.copy_from_slice(secret.as_slice());
        Ok(Self {
            address,
            fingerprint,
            token,
        })
    }
}
fn validate_address(address: SocketAddr) -> io::Result<()> {
    if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
        Err(invalid("Use a specific sharing address and port"))
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_round_trip_and_debug_redacts_permissions() {
        let identity = ShareIdentity::generate().unwrap();
        let invitation = identity
            .invitation("127.0.0.1:5000".parse().unwrap())
            .unwrap();
        let link = invitation.to_link();
        let parsed: Invitation = link.parse().unwrap();
        assert_eq!(parsed.to_link(), link);
        assert!(!format!("{parsed:?}").contains(link.split('/').next_back().unwrap()));
        assert!(!format!("{identity:?}").contains(link.split('/').next_back().unwrap()));
    }
    #[test]
    fn malformed_links_never_become_plaintext_addresses() {
        for value in [
            "127.0.0.1:5000",
            "mightymonitor://0.0.0.0:5000/a/b",
            "mightymonitor://127.0.0.1:0/a/b",
            "mightymonitor://127.0.0.1:5000/a/b/extra",
        ] {
            assert!(value.parse::<Invitation>().is_err());
        }
    }
    #[test]
    fn new_sessions_revoke_previous_tokens_and_identity() {
        let first = ShareIdentity::generate().unwrap();
        let second = ShareIdentity::generate().unwrap();
        let invite = first.invitation("127.0.0.1:5000".parse().unwrap()).unwrap();
        assert!(first.authenticate(&invite.authentication()));
        assert!(!second.authenticate(&invite.authentication()));
        assert!(second
            .invitation(invite.address)
            .unwrap()
            .client_config(first.certificate.as_ref().to_vec())
            .is_err());
    }
}

/// A secure invitation or an explicitly allowed legacy development address.
#[derive(Clone, Debug)]
pub enum Endpoint {
    Secure(Invitation),
    Legacy(SocketAddr),
    Paired {
        address: SocketAddr,
        identity: Arc<pairing::DeviceIdentity>,
        peer: store::Peer,
    },
}
impl Endpoint {
    pub fn parse(value: &str, allow_legacy: bool) -> io::Result<Self> {
        let value = value.trim();
        if let Some(value) = value.strip_prefix("paired://") {
            let (id, route) = value
                .split_once('@')
                .ok_or_else(|| invalid("Use paired://ID@IP:PORT"))?;
            let address: SocketAddr = route
                .parse()
                .map_err(|_| invalid("Invalid paired address"))?;
            validate_address(address)?;
            let directory = store::config_dir()?;
            let peer = store::DeviceStore::open(&directory)?.find(id)?;
            let identity = Arc::new(pairing::DeviceIdentity::open(&directory)?);
            return Ok(Self::Paired {
                address,
                identity,
                peer,
            });
        }
        if value.starts_with("mightymonitor://") {
            return Ok(Self::Secure(value.parse()?));
        }
        if !allow_legacy {
            return Err(invalid("Paste the connection link from the sharing device"));
        }
        let address: SocketAddr = value
            .parse()
            .map_err(|_| invalid("Enter a device IP:port or a connection link"))?;
        validate_address(address)?;
        Ok(Self::Legacy(address))
    }
    pub fn address(&self) -> SocketAddr {
        match self {
            Self::Secure(i) => i.address(),
            Self::Legacy(a) => *a,
            Self::Paired { address, .. } => *address,
        }
    }
    pub fn is_secure(&self) -> bool {
        matches!(self, Self::Secure(_) | Self::Paired { .. })
    }
}

pub mod managed;
