//! Transport-established authority used to bind resumable host sessions.
// Used only by the desktop `async` pairing paths; android-bridge builds without it.
#![cfg_attr(not(feature = "async"), allow(dead_code))]
use super::*;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
enum Principal {
    PrivateLink([u8; 32]),
    Paired {
        fingerprint: [u8; 32],
        credential: [u8; 32],
        directory: PathBuf,
    },
}
/// Constructed only after transport authentication. Debug omits identity data.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthenticatedPeer(Principal);
impl fmt::Debug for AuthenticatedPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthenticatedPeer { [redacted] }")
    }
}
impl AuthenticatedPeer {
    pub(crate) fn paired(fingerprint: [u8; 32], secret: &[u8], directory: PathBuf) -> Self {
        Self(Principal::Paired {
            fingerprint,
            credential: Sha256::digest(secret).into(),
            directory,
        })
    }
    /// Fail closed on store errors, revocation, or replacement credentials.
    pub fn is_current(&self) -> bool {
        match &self.0 {
            Principal::PrivateLink(_) => true,
            Principal::Paired {
                fingerprint,
                credential,
                directory,
            } => store::DeviceStore::open(directory).is_ok_and(|s| {
                s.peers().iter().any(|p| {
                    p.fingerprint == *fingerprint
                        && bool::from(
                            Sha256::digest(p.secret.as_slice())
                                .as_slice()
                                .ct_eq(credential),
                        )
                })
            }),
        }
    }
}
impl ShareIdentity {
    pub(crate) fn authenticated_peer(&self) -> AuthenticatedPeer {
        let mut hash = Sha256::new();
        hash.update(self.fingerprint);
        hash.update(self.token.as_slice());
        AuthenticatedPeer(Principal::PrivateLink(hash.finalize().into()))
    }
}
/// Cryptographically unpredictable protocol session identifier.
pub fn random_session_id() -> io::Result<u64> {
    let mut bytes = [0; 8];
    getrandom::getrandom(&mut bytes).map_err(tls_error)?;
    Ok(u64::from_le_bytes(bytes))
}
