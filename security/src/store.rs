//! Bounded, private, atomically replaced device credentials. Corruption fails closed.
use super::*;
use crate::pairing::Clock;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_FILE: u64 = 16384;
pub const MAX_PEERS: usize = 64;

pub fn config_dir() -> io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(target_os = "windows"))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
    base.filter(|p| p.is_absolute())
        .map(|p| p.join("mightymonitor"))
        .ok_or_else(|| invalid("No absolute configuration directory available"))
}
pub(crate) fn read_private(path: &Path) -> io::Result<Zeroizing<Vec<u8>>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_FILE {
        return Err(invalid("Credential file too large"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(denied());
        }
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(invalid("Credential file too large"));
    }
    Ok(bytes)
}
/// Callers serialize writes; an exclusive sibling lock rejects simultaneous writers.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_checked(path, bytes, None)
}
pub(crate) fn atomic_create(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_checked(path, bytes, Some(None))
}
fn atomic_write_checked(
    path: &Path,
    bytes: &[u8],
    expected: Option<Option<[u8; 32]>>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Missing credential directory"))?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = path.with_extension("pending");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        if let Some(expected) = expected {
            let actual = match read_private(path) {
                Ok(data) => Some(Sha256::digest(data.as_slice()).into()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                Err(e) => return Err(e),
            };
            if actual != expected {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "Credentials changed; reopen the store",
                ));
            }
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
#[derive(Clone)]
pub struct Peer {
    pub fingerprint: [u8; 32],
    pub(crate) secret: Zeroizing<[u8; 32]>,
    pub(crate) name: String,
    pub(crate) paired_at: u64,
    pub(crate) last_seen: u64,
}
impl fmt::Debug for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Peer { credentials: [redacted] }")
    }
}
/// Display-only paired-device model. Credentials and routes are never exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedDevice {
    pub device_id: String,
    pub friendly_name: String,
    pub paired_at: u64,
    pub last_seen: u64,
}
impl Peer {
    pub(crate) fn new(fingerprint: [u8; 32], secret: Zeroizing<[u8; 32]>, name: String) -> Self {
        let now = crate::pairing::SystemClock.now();
        Self {
            fingerprint,
            secret,
            name,
            paired_at: now,
            last_seen: now,
        }
    }

    pub fn id(&self) -> String {
        short_id(&self.fingerprint)
    }
}
pub fn short_id(fingerprint: &[u8; 32]) -> String {
    fingerprint[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub struct DeviceStore {
    path: PathBuf,
    peers: Vec<Peer>,
    revision: Option<[u8; 32]>,
}
impl fmt::Debug for DeviceStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceStore { credentials: [redacted] }")
    }
}
impl DeviceStore {
    pub fn open(directory: &Path) -> io::Result<Self> {
        let path = directory.join("peers.bin");
        let bytes = match read_private(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    peers: Vec::new(),
                    revision: None,
                })
            }
            Err(e) => return Err(e),
        };
        if !bytes.starts_with(b"MMPEER2\0")
            || (bytes.len() - 8) % 209 != 0
            || (bytes.len() - 8) / 209 > MAX_PEERS
        {
            return Err(invalid("Invalid paired-device store"));
        }
        let mut peers = Vec::new();
        for record in bytes[8..].chunks_exact(209) {
            let mut fingerprint = [0; 32];
            fingerprint.copy_from_slice(&record[..32]);
            if peers.iter().any(|p: &Peer| p.fingerprint == fingerprint) {
                return Err(invalid("Duplicate peer"));
            }
            let mut secret = Zeroizing::new([0; 32]);
            secret.copy_from_slice(&record[32..64]);
            let length = record[64] as usize;
            if length == 0 || length > 128 || record[65 + length..193].iter().any(|b| *b != 0) {
                return Err(invalid("Invalid device name"));
            }
            let name = std::str::from_utf8(&record[65..65 + length])
                .map_err(|_| invalid("Invalid device name"))?
                .to_string();
            if crate::pairing::sanitize_name(&name) != name {
                return Err(invalid("Invalid device name"));
            }
            let paired_at = u64::from_le_bytes(record[193..201].try_into().map_err(tls_error)?);
            let last_seen = u64::from_le_bytes(record[201..209].try_into().map_err(tls_error)?);
            peers.push(Peer {
                fingerprint,
                secret,
                name,
                paired_at,
                last_seen,
            });
        }
        Ok(Self {
            path,
            peers,
            revision: Some(Sha256::digest(bytes.as_slice()).into()),
        })
    }
    pub fn list(&self) -> Vec<PairedDevice> {
        self.peers
            .iter()
            .map(|p| PairedDevice {
                device_id: p.id(),
                friendly_name: p.name.clone(),
                paired_at: p.paired_at,
                last_seen: p.last_seen,
            })
            .collect()
    }
    pub fn forget(&mut self, id: &str) -> io::Result<bool> {
        if !self.peers.iter().any(|p| p.id() == id) {
            return Ok(false);
        }
        self.unpair(id)?;
        Ok(true)
    }
    pub fn forget_all(&mut self) -> io::Result<()> {
        self.save(&[])?;
        self.peers.clear();
        Ok(())
    }
    pub(crate) fn seen(&mut self, fingerprint: &[u8; 32]) -> io::Result<()> {
        let mut next = self.peers.clone();
        if let Some(p) = next.iter_mut().find(|p| &p.fingerprint == fingerprint) {
            p.last_seen = crate::pairing::SystemClock.now();
        }
        self.save(&next)?;
        self.peers = next;
        Ok(())
    }
    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }
    pub fn find(&self, id: &str) -> io::Result<Peer> {
        let mut matching = self.peers.iter().filter(|p| p.id() == id);
        let peer = matching.next().ok_or_else(denied)?;
        if matching.next().is_some() {
            return Err(denied());
        }
        Ok(peer.clone())
    }
    pub fn insert(&mut self, peer: Peer) -> io::Result<()> {
        let mut next = self.peers.clone();
        if let Some(old) = next.iter_mut().find(|p| p.fingerprint == peer.fingerprint) {
            *old = peer;
        } else {
            if next.len() >= MAX_PEERS {
                return Err(invalid("Paired-device limit reached"));
            }
            next.push(peer);
        }
        self.save(&next)?;
        self.peers = next;
        Ok(())
    }
    pub fn unpair(&mut self, id: &str) -> io::Result<()> {
        self.find(id)?;
        let next: Vec<_> = self
            .peers
            .iter()
            .filter(|p| p.id() != id)
            .cloned()
            .collect();
        self.save(&next)?;
        self.peers = next;
        Ok(())
    }
    fn save(&mut self, peers: &[Peer]) -> io::Result<()> {
        let mut bytes = Zeroizing::new(b"MMPEER2\0".to_vec());
        for peer in peers {
            bytes.extend_from_slice(&peer.fingerprint);
            bytes.extend_from_slice(peer.secret.as_slice());
            let name = crate::pairing::sanitize_name(&peer.name);
            bytes.push(name.len() as u8);
            bytes.extend_from_slice(name.as_bytes());
            let padded_len = bytes.len() + 128 - name.len();
            bytes.resize(padded_len, 0);
            bytes.extend_from_slice(&peer.paired_at.to_le_bytes());
            bytes.extend_from_slice(&peer.last_seen.to_le_bytes());
        }
        atomic_write_checked(&self.path, &bytes, Some(self.revision))?;
        self.revision = Some(Sha256::digest(bytes.as_slice()).into());
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub struct Directory(pub PathBuf);
    impl Directory {
        pub fn new() -> Self {
            let mut random = [0; 16];
            getrandom::getrandom(&mut random).unwrap();
            let suffix = URL_SAFE_NO_PAD.encode(random);
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../target/m8-tests")
                .join(suffix);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn persistence_bounds_revocation_atomicity_and_redaction() {
        let dir = Directory::new();
        let mut store = DeviceStore::open(&dir.0).unwrap();
        for n in 0..MAX_PEERS {
            let mut fingerprint = [0; 32];
            fingerprint[0] = n as u8;
            store
                .insert(Peer {
                    fingerprint,
                    secret: Zeroizing::new([123; 32]),
                    name: "Device".into(),
                    paired_at: 0,
                    last_seen: 0,
                })
                .unwrap();
        }
        assert_eq!(DeviceStore::open(&dir.0).unwrap().peers().len(), MAX_PEERS);
        assert!(store
            .insert(Peer {
                fingerprint: [255; 32],
                secret: Zeroizing::new([123; 32]),
                name: "Device".into(),
                paired_at: 0,
                last_seen: 0,
            })
            .is_err());
        let id = store.peers()[0].id();
        store.unpair(&id).unwrap();
        assert!(DeviceStore::open(&dir.0).unwrap().find(&id).is_err());
        let before = read_private(&dir.0.join("peers.bin")).unwrap();
        fs::write(dir.0.join("peers.pending"), b"occupied").unwrap();
        assert!(store.unpair(&store.peers()[0].id()).is_err());
        assert_eq!(*before, *read_private(&dir.0.join("peers.bin")).unwrap());
        assert!(!format!("{store:?}").contains(&URL_SAFE_NO_PAD.encode([123; 32])));
        assert!(!format!("{:?}", store.peers()[0]).contains("123"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.0.join("peers.bin"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn stale_writer_cannot_restore_revoked_peer() {
        let dir = Directory::new();
        let mut current = DeviceStore::open(&dir.0).unwrap();
        let peer = Peer {
            fingerprint: [1; 32],
            secret: Zeroizing::new([42; 32]),
            name: "Device".into(),
            paired_at: 0,
            last_seen: 0,
        };
        current.insert(peer.clone()).unwrap();
        let mut stale = DeviceStore::open(&dir.0).unwrap();
        current.unpair(&peer.id()).unwrap();
        assert!(stale
            .insert(Peer {
                fingerprint: [2; 32],
                secret: Zeroizing::new([43; 32]),
                name: "Device".into(),
                paired_at: 0,
                last_seen: 0,
            })
            .is_err());
        assert!(DeviceStore::open(&dir.0).unwrap().peers().is_empty());
    }
    #[test]
    fn corrupt_and_oversized_files_fail_closed() {
        let dir = Directory::new();
        let path = dir.0.join("peers.bin");
        for bytes in [
            vec![],
            b"MMPEER1\0x".to_vec(),
            vec![0; MAX_FILE as usize + 1],
        ] {
            atomic_write(&path, &bytes).unwrap();
            assert!(DeviceStore::open(&dir.0).is_err());
        }
    }
    #[test]
    fn duplicate_records_are_rejected() {
        let dir = Directory::new();
        let mut store = DeviceStore::open(&dir.0).unwrap();
        store
            .insert(Peer::new([1; 32], Zeroizing::new([42; 32]), "Room".into()))
            .unwrap();
        let mut bytes = read_private(&dir.0.join("peers.bin")).unwrap();
        let record = bytes[8..].to_vec();
        bytes.extend_from_slice(&record);
        atomic_write(&dir.0.join("peers.bin"), &bytes).unwrap();
        assert!(DeviceStore::open(&dir.0).is_err());
    }
    #[test]
    fn identity_survives_reopen_and_corruption_is_terminal() {
        let dir = Directory::new();
        let first = crate::pairing::DeviceIdentity::open(&dir.0).unwrap();
        assert_eq!(
            first.fingerprint(),
            crate::pairing::DeviceIdentity::open(&dir.0)
                .unwrap()
                .fingerprint()
        );
        assert!(format!("{first:?}").contains("redacted"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.0.join("identity.bin"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        atomic_write(&dir.0.join("identity.bin"), b"broken").unwrap();
        assert!(crate::pairing::DeviceIdentity::open(&dir.0).is_err());
    }
}
