use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use trinix_discovery::{txt::*, *};
#[derive(Default)]
struct State {
    registered: bool,
    unregistered: bool,
    shutdown: bool,
    browsing: bool,
    events: VecDeque<Event>,
    fail_register: bool,
    fail_next: bool,
    strict_addresses: bool,
}
struct Fake(Arc<Mutex<State>>);
impl Backend for Fake {
    fn local_interfaces(&self) -> std::io::Result<Vec<LocalInterface>> {
        Ok(vec![("192.168.10.1".parse().unwrap(), 24)])
    }
    fn allow_loopback_for_tests(&self) -> bool {
        !self.0.lock().unwrap().strict_addresses
    }

    fn register(&mut self, _: &Metadata, _: SocketAddr) -> std::io::Result<String> {
        if self.0.lock().unwrap().fail_register {
            return Err(std::io::Error::other("test registration failure"));
        }
        self.0.lock().unwrap().registered = true;
        Ok("instance".into())
    }
    fn unregister(&mut self, _: &str) {
        self.0.lock().unwrap().unregistered = true;
    }
    fn shutdown(&mut self) {
        self.0.lock().unwrap().shutdown = true;
    }
    fn start_browse(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().browsing = true;
        Ok(())
    }
    fn stop_browse(&mut self) {
        self.0.lock().unwrap().browsing = false;
    }
    fn next(&mut self, _: Duration) -> std::io::Result<Option<Event>> {
        if self.0.lock().unwrap().fail_next {
            return Err(std::io::Error::other("test browse failure"));
        }
        Ok(self.0.lock().unwrap().events.pop_front())
    }
}
fn metadata() -> Metadata {
    Metadata::new("Desk", "0123456789abcdef").unwrap()
}
fn device() -> Device {
    Device {
        metadata: metadata(),
        addresses: vec!["127.0.0.1:5000".parse().unwrap()],
    }
}
#[test]
fn registration_guard_drops_unregister_and_shutdown() {
    let state = Arc::new(Mutex::new(State::default()));
    let ad = Advertisement::start(Fake(state.clone()), &metadata(), device().addresses[0]).unwrap();
    assert!(state.lock().unwrap().registered);
    drop(ad);
    let state = state.lock().unwrap();
    assert!(state.unregistered && state.shutdown);
}
#[test]
fn resolution_dedupe_removal_expiry_and_bounds() {
    let mut nearby = Nearby::default();
    let now = Instant::now();
    nearby.apply(Event::Resolved("one".into(), device()), now);
    nearby.apply(Event::Resolved("one".into(), device()), now);
    assert_eq!(nearby.devices().len(), 1);
    nearby.apply(Event::Removed("one".into()), now);
    assert!(nearby.devices().is_empty());
    for n in 0..100 {
        nearby.apply(Event::Resolved(n.to_string(), device()), now);
    }
    assert_eq!(nearby.devices().len(), MAX_RESULTS);
    nearby.expire(now + Duration::from_secs(121));
    assert!(nearby.devices().is_empty());
    let state = Arc::new(Mutex::new(State::default()));
    state
        .lock()
        .unwrap()
        .events
        .push_back(Event::Resolved("one".into(), device()));
    assert_eq!(
        browse(&mut Fake(state.clone()), Duration::from_millis(20))
            .unwrap()
            .len(),
        1
    );
    assert!(!state.lock().unwrap().browsing);
}
#[test]
fn txt_strict_validation() {
    assert_eq!(*WIRE, trinix_core::PROTOCOL_VERSION.to_string());
    let records = metadata().records();
    assert_eq!(records["wire"], trinix_core::PROTOCOL_VERSION.to_string());
    let different = (u64::from(trinix_core::PROTOCOL_VERSION) + 1).to_string();
    let mut bytes: Vec<_> = records
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_bytes()))
        .collect();
    assert_eq!(parse(bytes.clone()).unwrap(), metadata());
    bytes.push(("extra", b"ignored"));
    assert!(parse(bytes.clone()).is_ok());
    bytes.push(("NAME", b"duplicate"));
    assert!(parse(bytes).is_err());
    for (key, value) in [
        ("wire", different.as_bytes()),
        ("name", b"bad\nname"),
        ("id", b"not-hex"),
        ("name", &[255]),
    ] {
        let mut input: Vec<_> = records
            .iter()
            .filter(|(k, _)| k.as_str() != key)
            .map(|(k, v)| (k.as_str(), v.as_bytes()))
            .collect();
        input.push((key, value));
        assert!(parse(input).is_err());
    }
    assert!(parse([("name", vec![b'a'; 129].as_slice())]).is_err());
    assert!(parse([]).is_err());
    assert_eq!(sanitize_name(" /bad\n name💚"), "bad name");
    assert!(sanitize_name(&"a".repeat(100)).len() <= 40);
}
#[test]
fn advertisements_never_contain_private_link_or_credentials() {
    use base64::{
        engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
        Engine,
    };
    use trinix_security::ShareIdentity;
    let share = ShareIdentity::generate().unwrap();
    let invitation = share.invitation("127.0.0.1:5000".parse().unwrap()).unwrap();
    let link = invitation.to_link();
    let parts: Vec<_> = link.split('/').collect();
    let fingerprint = URL_SAFE_NO_PAD.decode(parts[3]).unwrap();
    let token = URL_SAFE_NO_PAD.decode(parts[4]).unwrap();
    // The caller supplies an opaque routing ID, never a persistent identity or credential.
    let short_id = fingerprint[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let metadata = Metadata::new("Test desk", &short_id).unwrap();
    // The session private key is intentionally inaccessible. Also check real
    // independently generated PKCS#8 key material as a regression fixture.
    let key_fixture = rcgen::KeyPair::generate().unwrap().serialize_der();
    let mut advertised = Vec::new();
    for (key, value) in metadata.records() {
        assert!(KEYS.contains(&key.as_str()));
        advertised.extend_from_slice(key.as_bytes());
        advertised.extend_from_slice(value.as_bytes());
        advertised.push(0);
    }
    advertised.extend_from_slice(metadata.instance().as_bytes());
    advertised.extend_from_slice(metadata.hostname().as_bytes());
    for secret in [&fingerprint, &token, &key_fixture] {
        let mut encodings = vec![
            secret.clone(),
            secret
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
            secret
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
                .into_bytes(),
        ];
        for engine in [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD] {
            encodings.push(engine.encode(secret).into_bytes());
        }
        for encoding in encodings {
            assert!(!advertised.windows(encoding.len()).any(|w| w == encoding));
        }
    }
    let text = String::from_utf8(advertised.clone()).unwrap();
    assert!(!text.contains(&link));
    assert!(!text.contains("mightymonitor://"));
    assert!(!text.contains("PRIVATE KEY"));
    // ShareIdentity deliberately exposes no private key. The closed schema has
    // only short ASCII fields: neither a DER key nor its base64/hex encoding fits.
    assert!(advertised.iter().all(u8::is_ascii));
    assert!(metadata.records().values().all(|v| v.len() <= 40));
    assert!(metadata.instance().len() <= 57);
    assert!(metadata.hostname().len() <= 26);
}

#[test]
fn rejects_injected_and_oversized_records_and_ids() {
    for id in ["", "A", "0123456789abcdef0", "secret!"] {
        assert!(Metadata::new("Desk", id).is_err());
    }
    assert!(Metadata::new("Desk", "a").is_ok());
    let records = metadata().records();
    let base: Vec<_> = records
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_bytes()))
        .collect();
    for extra in [
        vec![("wire=invalid", b"injected".as_slice())],
        vec![("unknown", b"x".as_slice()), ("UNKNOWN", b"x".as_slice())],
        vec![("unknown", &[255][..])],
    ] {
        let mut input = base.clone();
        input.extend(extra);
        assert!(parse(input).is_err());
    }
    assert!(parse([("a".repeat(33).as_str(), b"x".as_slice())]).is_err());
    let values: Vec<_> = (0..10)
        .map(|n| (format!("extra{n}"), vec![b'a'; 128]))
        .collect();
    assert!(parse(values.iter().map(|(k, v)| (k.as_str(), v.as_slice()))).is_err());
    let state = Arc::new(Mutex::new(State::default()));
    assert!(browse(&mut Fake(state.clone()), Duration::from_secs(31)).is_err());
    assert!(!state.lock().unwrap().browsing);
}

#[test]
fn failures_release_backend_resources() {
    let state = Arc::new(Mutex::new(State {
        fail_register: true,
        ..State::default()
    }));
    assert!(Advertisement::start(Fake(state.clone()), &metadata(), device().addresses[0]).is_err());
    assert!(state.lock().unwrap().shutdown);
    let state = Arc::new(Mutex::new(State {
        fail_next: true,
        ..State::default()
    }));
    assert!(browse(&mut Fake(state.clone()), Duration::from_millis(20)).is_err());
    assert!(!state.lock().unwrap().browsing);
}

#[test]
fn address_policy_table() {
    let interfaces = [
        ("192.168.10.1".parse().unwrap(), 24),
        ("169.254.2.1".parse().unwrap(), 24),
        ("fd12:3456::1".parse().unwrap(), 64),
    ];
    for (ip, allowed) in [
        ("127.0.0.1", false),
        ("::1", false),
        ("8.8.8.8", false),
        ("2001:4860:4860::8888", false),
        ("0.0.0.0", false),
        ("::", false),
        ("224.0.0.1", false),
        ("ff02::1", false),
        ("255.255.255.255", false),
        ("192.168.10.255", false),
        ("fe80::1", false),
        ("169.254.3.1", false),
        ("169.254.2.2", true),
        ("192.168.11.2", false),
        ("10.0.0.1", false),
        ("172.16.0.1", false),
        ("192.168.10.2", true),
        ("fd12:3456::2", true),
        ("fd12:3457::2", false),
        ("fc00::1", false),
    ] {
        assert_eq!(
            address_allowed(ip.parse().unwrap(), &interfaces),
            allowed,
            "{ip}"
        );
    }
    assert!(!address_allowed("192.168.10.2".parse().unwrap(), &[]));
}

#[test]
fn fake_browse_filters_addresses_and_drops_empty_devices() {
    let state = Arc::new(Mutex::new(State {
        strict_addresses: true,
        ..State::default()
    }));
    for (name, addresses) in [
        (
            "mixed",
            vec!["8.8.8.8:5000", "192.168.11.2:5000", "192.168.10.2:5000"],
        ),
        ("invalid", vec!["[::1]:5000", "[2001:db8::1]:5000"]),
    ] {
        let addresses = addresses.into_iter().map(|s| s.parse().unwrap()).collect();
        state.lock().unwrap().events.push_back(Event::Resolved(
            name.into(),
            Device {
                metadata: metadata(),
                addresses,
            },
        ));
    }
    let devices = browse(&mut Fake(state), Duration::from_millis(20)).unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(
        devices[0].addresses,
        vec!["192.168.10.2:5000".parse::<SocketAddr>().unwrap()]
    );
}

#[test]
fn registration_rejects_nonqualifying_routes() {
    for address in [
        "8.8.8.8:5000",
        "192.168.11.2:5000",
        "192.168.10.255:5000",
        "0.0.0.0:5000",
        "[fe80::1]:5000",
    ] {
        let state = Arc::new(Mutex::new(State::default()));
        assert!(
            Advertisement::start(Fake(state.clone()), &metadata(), address.parse().unwrap())
                .is_err()
        );
        assert!(!state.lock().unwrap().registered);
        assert!(state.lock().unwrap().shutdown);
    }
    let mut backend = Mdns::new().unwrap();
    assert!(backend
        .register(&metadata(), "127.0.0.1:5000".parse().unwrap())
        .is_err());
}
