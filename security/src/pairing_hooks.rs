//! UI-independent display names, code-entry limits and single-use QR offers.
use super::*;
use std::sync::Mutex;
/// Seconds since the Unix epoch. Services retain one clock across reconnects.
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
/// Names never carry route information. Invalid names become a neutral label.
pub fn sanitize_name(value: &str) -> String {
    if value_contains_port(value) {
        return "Device".into();
    }
    let value: String = value
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        .take(32)
        .collect();
    // Reject the original too: filtering ':' must not disguise host:port input.
    let ip_like = value.split(|c: char| !c.is_numeric() && c != '.').any(|s| {
        s.split('.')
            .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_numeric()))
            .count()
            >= 4
    });
    let port_like = value_contains_port(value.as_str());
    if value.trim().is_empty() || ip_like || port_like {
        "Device".into()
    } else {
        value.trim().into()
    }
}
fn value_contains_port(value: &str) -> bool {
    value
        .split(':')
        .skip(1)
        .any(|s| s.chars().take_while(|c| c.is_numeric()).count() >= 2)
}
pub(crate) fn default_name() -> String {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok();
    #[cfg(unix)]
    let name = name.or_else(|| {
        use std::io::Read;
        let mut s = String::new();
        std::fs::File::open("/etc/hostname")
            .ok()?
            .take(256)
            .read_to_string(&mut s)
            .ok()?;
        Some(s.trim().into())
    });
    sanitize_name(name.as_deref().unwrap_or("Device"))
}
/// Six decimal digits sampled without modulo bias. Debug redacts the code.
pub struct PairingCode(Zeroizing<String>);
impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode { [redacted] }")
    }
}
impl PairingCode {
    pub fn generate() -> io::Result<Self> {
        loop {
            let mut bytes = [0; 4];
            getrandom::getrandom(&mut bytes).map_err(tls_error)?;
            let n = u32::from_le_bytes(bytes);
            if n < u32::MAX - (u32::MAX % 1_000_000) {
                return Ok(Self(Zeroizing::new(format!("{:06}", n % 1_000_000))));
            }
        }
    }
    /// For intentional local display/entry only; never diagnostics.
    pub fn expose(&self) -> &str {
        &self.0
    }
}
struct Attempt {
    code: PairingCode,
    expires: u64,
    failures: u8,
    used: bool,
    restart_after: u64,
}
/// Host-owned across connections. Three failures lock the offer; only a local
/// restart after cooldown creates a fresh code. This is separate from TLS SAS.
pub struct PairingAttemptLimiter {
    clock: Arc<dyn Clock>,
    state: Mutex<Attempt>,
}
impl PairingAttemptLimiter {
    pub fn new(clock: Arc<dyn Clock>) -> io::Result<Self> {
        let now = clock.now();
        Ok(Self {
            clock,
            state: Mutex::new(Attempt {
                code: PairingCode::generate()?,
                expires: now.saturating_add(60),
                failures: 0,
                used: false,
                restart_after: now.saturating_add(60),
            }),
        })
    }
    /// Reject before certificate processing once the shared window is locked.
    pub fn check_open(&self) -> io::Result<()> {
        let state = self.state.lock().map_err(|_| denied())?;
        if state.failures >= 3 || self.clock.now() >= state.expires {
            return Err(denied());
        }
        Ok(())
    }
    /// Count a failed confirmation, malformed request or rejected offer.
    pub fn failed_attempt(&self) -> io::Result<()> {
        let mut state = self.state.lock().map_err(|_| denied())?;
        state.failures = state.failures.saturating_add(1);
        Ok(())
    }
    pub fn locked(&self) -> bool {
        self.state.lock().map_or(true, |state| state.failures >= 3)
    }
    pub fn display_code(&self) -> Zeroizing<String> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .code
            .0
            .clone()
    }
    pub fn verify(&self, input: &str) -> io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut candidate = [0; 6];
        if input.len() == 6 {
            candidate.copy_from_slice(input.as_bytes());
        }
        let equal = bool::from(candidate.ct_eq(state.code.expose().as_bytes()));
        if state.used || state.failures >= 3 || self.clock.now() >= state.expires {
            return Err(denied());
        }
        if !equal {
            state.failures += 1;
            return Err(denied());
        }
        state.used = true;
        Ok(())
    }
    /// Invoke only for an explicit local start, never on reconnect.
    pub fn restart(&self) -> io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let now = self.clock.now();
        if now < state.restart_after {
            return Err(denied());
        }
        *state = Attempt {
            code: PairingCode::generate()?,
            expires: now.saturating_add(60),
            failures: 0,
            used: false,
            restart_after: now.saturating_add(60),
        };
        Ok(())
    }
}
/// A routing/pinning hint, never an authorization or a long-lived credential.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingOffer {
    pub(crate) nonce: [u8; 16],
    pub(crate) fingerprint: [u8; 32],
    address: SocketAddr,
    expires: u64,
}
impl fmt::Debug for PairingOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingOffer { [redacted] }")
    }
}
impl PairingOffer {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }
    pub fn expires_at(&self) -> u64 {
        self.expires
    }
    pub fn to_offer_string(&self) -> String {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.fingerprint);
        bytes.extend_from_slice(&self.expires.to_le_bytes());
        bytes.extend_from_slice(self.address.to_string().as_bytes());
        format!("mightymonitor-pair://{}", URL_SAFE_NO_PAD.encode(bytes))
    }
    pub fn parse(value: &str, clock: &dyn Clock) -> io::Result<Self> {
        // A single error deliberately hides which field was invalid.
        let parse = || -> Option<Self> {
            if value.len() > 256 {
                return None;
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(value.strip_prefix("mightymonitor-pair://")?)
                .ok()?;
            if bytes.len() < 60 || bytes[0] != 1 {
                return None;
            }
            let expires = u64::from_le_bytes(bytes[49..57].try_into().ok()?);
            if expires <= clock.now() || expires > clock.now().saturating_add(120) {
                return None;
            }
            let address = std::str::from_utf8(&bytes[57..]).ok()?.parse().ok()?;
            validate_address(address).ok()?;
            Some(Self {
                nonce: bytes[1..17].try_into().ok()?,
                fingerprint: bytes[17..49].try_into().ok()?,
                address,
                expires,
            })
        };
        parse().ok_or_else(denied)
    }
}
/// One outstanding offer per locally enabled mode. Replacement revokes the old
/// offer. Taking it is atomic and consumes it before validation or TLS consent.
pub(crate) struct Offers {
    clock: Arc<dyn Clock>,
    pending: Mutex<Option<PairingOffer>>,
    end: u64,
}
impl Offers {
    pub(crate) fn new(clock: Arc<dyn Clock>) -> Self {
        let end = clock.now().saturating_add(60);
        Self {
            clock,
            pending: Mutex::new(None),
            end,
        }
    }
    pub(crate) fn create(
        &self,
        fingerprint: [u8; 32],
        address: SocketAddr,
    ) -> io::Result<PairingOffer> {
        validate_address(address)?;
        if self.clock.now() >= self.end {
            return Err(denied());
        }
        let mut nonce = [0; 16];
        getrandom::getrandom(&mut nonce).map_err(tls_error)?;
        let offer = PairingOffer {
            nonce,
            fingerprint,
            address,
            expires: self.end,
        };
        *self.pending.lock().unwrap_or_else(|p| p.into_inner()) = Some(offer.clone());
        Ok(offer)
    }
    pub(crate) fn consume(&self, nonce: &[u8; 16]) -> io::Result<()> {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        let offer = pending.take().ok_or_else(denied)?;
        if self.clock.now() >= offer.expires || !bool::from(offer.nonce.ct_eq(nonce)) {
            return Err(denied());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pairing::{DeviceIdentity, PairingMode},
        store::{tests::Directory, DeviceStore},
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    struct FakeClock(AtomicU64);
    impl Clock for FakeClock {
        fn now(&self) -> u64 {
            self.0.load(Ordering::Relaxed)
        }
    }
    #[test]
    fn code_sampling_lockout_restart_and_expiry() {
        let mut buckets = [0; 10];
        for _ in 0..10_000 {
            let c = PairingCode::generate().unwrap();
            assert_eq!(c.expose().len(), 6);
            assert!(c.expose().bytes().all(|b| b.is_ascii_digit()));
            buckets[(c.expose().as_bytes()[0] - b'0') as usize] += 1;
            assert!(!format!("{c:?}").contains(c.expose()));
        }
        assert!(buckets.iter().all(|n| (700..1300).contains(n)));
        let clock = Arc::new(FakeClock(AtomicU64::new(100)));
        let limiter = PairingAttemptLimiter::new(clock.clone()).unwrap();
        let correct = limiter.display_code();
        for _ in 0..3 {
            assert!(limiter.verify("invalid").is_err());
        }
        assert!(limiter.verify(&correct).is_err());
        assert!(limiter.restart().is_err());
        clock.0.store(161, Ordering::Relaxed);
        limiter.restart().unwrap();
        assert!(limiter.verify(&limiter.display_code()).is_ok());
        assert!(limiter.verify(&limiter.display_code()).is_err());
        clock.0.store(222, Ordering::Relaxed);
        limiter.restart().unwrap();
        clock.0.store(283, Ordering::Relaxed);
        assert!(limiter.verify(&limiter.display_code()).is_err());
    }
    #[test]
    fn offer_roundtrip_single_use_concurrency_expiry_and_no_credentials() {
        let dir = Directory::new();
        let identity = DeviceIdentity::open(&dir.0).unwrap();
        let clock = Arc::new(FakeClock(AtomicU64::new(100)));
        let mode = PairingMode::with_clock(clock.clone());
        let offer = mode
            .create_offer(&identity, "127.0.0.1:5000".parse().unwrap())
            .unwrap();
        let encoded = offer.to_offer_string();
        let parsed = PairingOffer::parse(&encoded, clock.as_ref()).unwrap();
        assert_eq!(offer, parsed);
        let decoded = URL_SAFE_NO_PAD
            .decode(encoded.strip_prefix("mightymonitor-pair://").unwrap())
            .unwrap();
        let key = crate::store::read_private(&dir.0.join("identity.bin")).unwrap();
        let certificate_len = u32::from_le_bytes(key[8..12].try_into().unwrap()) as usize;
        let private_key = &key[12 + certificate_len..];
        let share = ShareIdentity::generate().unwrap();
        for secret in [private_key, share.token.as_slice(), &[123; 32]] {
            assert!(!decoded.windows(secret.len()).any(|w| w == secret));
            assert!(!encoded.contains(&URL_SAFE_NO_PAD.encode(secret)));
            let hex: String = secret.iter().map(|b| format!("{b:02x}")).collect();
            assert!(!encoded.contains(&hex));
        }
        std::thread::scope(|scope| {
            let a = scope.spawn(|| mode.consume_offer(&offer).is_ok());
            let b = scope.spawn(|| mode.consume_offer(&offer).is_ok());
            assert_ne!(a.join().unwrap(), b.join().unwrap());
        });
        assert!(mode.consume_offer(&offer).is_err());
        let fresh = mode.create_offer(&identity, offer.address()).unwrap();
        let mut forged = fresh.clone();
        forged.nonce[0] ^= 1;
        assert!(mode.consume_offer(&forged).is_err());
        assert!(mode.consume_offer(&fresh).is_err());
        let expiring = mode.create_offer(&identity, offer.address()).unwrap();
        clock.0.store(161, Ordering::Relaxed);
        assert!(mode.consume_offer(&expiring).is_err());
        assert!(PairingOffer::parse(&encoded, clock.as_ref()).is_err());
        assert!(mode.create_offer(&identity, offer.address()).is_err());
        for invalid in ["", "mightymonitor-pair://!", &"x".repeat(257)] {
            assert!(PairingOffer::parse(invalid, clock.as_ref()).is_err());
        }
    }
    #[test]
    fn names_persist_and_display_model_has_no_routes() {
        let dir = Directory::new();
        let mut identity = DeviceIdentity::open(&dir.0).unwrap();
        for input in [
            "192.168.0.1",
            "room 999.1.2.3 screen",
            "computer:5000",
            "[::1]:5000",
            "x:12",
            "",
        ] {
            assert_eq!(sanitize_name(input), "Device");
        }
        identity.set_friendly_name("Living room").unwrap();
        assert_eq!(
            DeviceIdentity::open(&dir.0).unwrap().friendly_name(),
            "Living room"
        );
        let mut store = DeviceStore::open(&dir.0).unwrap();
        let peer = crate::store::Peer::new(
            [1; 32],
            Zeroizing::new([123; 32]),
            sanitize_name("10.20.30.40:5000"),
        );
        let id = peer.id();
        store.insert(peer).unwrap();
        let list = DeviceStore::open(&dir.0).unwrap().list();
        assert_eq!(list[0].friendly_name, "Device");
        assert!(store.forget(&id).unwrap());
        assert!(!store.forget(&id).unwrap());
        assert!(DeviceStore::open(&dir.0).unwrap().list().is_empty());
        store
            .insert(crate::store::Peer::new(
                [2; 32],
                Zeroizing::new([123; 32]),
                "Room".into(),
            ))
            .unwrap();
        store.forget_all().unwrap();
        assert!(DeviceStore::open(&dir.0).unwrap().list().is_empty());
    }
}
