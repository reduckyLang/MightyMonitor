//! Loopback authentication and server-side Resume authorization, without capture APIs.
use mightymonitor_core::{
    Bytes, Capture, Codec, CodecError, Encoder, FramedTransport, Message, Role, Transport,
    PROTOCOL_VERSION,
};
use mightymonitor_host::{
    pipeline::{HostEncoder, Pipeline},
    resume::ResumeRegistry,
    session::{serve_authenticated, SessionConfig},
    Error,
};
use mightymonitor_security::{
    asynchronous as private,
    pairing::{asynchronous as paired, DeviceIdentity, PairingMode},
    store::DeviceStore,
    AuthenticatedPeer, ShareIdentity,
};
use std::{path::PathBuf, time::Duration};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let name = mightymonitor_security::random_session_id().unwrap();
        Self(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../target/resume-tests/{name}")),
        )
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Source;
impl Capture for Source {
    fn next_frame(&mut self) -> Option<Bytes> {
        Some(vec![0; 16])
    }
}
struct Encode;
impl Encoder for Encode {
    fn encode(&mut self, b: &[u8]) -> Result<Bytes, CodecError> {
        Ok(b.into())
    }
    fn set_bitrate_kbps(&mut self, _: u32) -> Result<(), CodecError> {
        Ok(())
    }
}
impl HostEncoder for Encode {
    fn encode_video(&mut self, b: &[u8], _: u64, key: bool) -> Result<(Bytes, bool), Error> {
        Ok((b.into(), key))
    }
}
async fn sockets() -> (tokio::net::TcpStream, tokio::net::TcpStream) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (c, s) = tokio::join!(
        tokio::net::TcpStream::connect(l.local_addr().unwrap()),
        l.accept()
    );
    (c.unwrap(), s.unwrap().0)
}
async fn principal(identity: &ShareIdentity) -> AuthenticatedPeer {
    let (c, s) = sockets().await;
    let invite = identity
        .invitation("127.0.0.1:5000".parse().unwrap())
        .unwrap();
    let (h, v) = tokio::join!(
        private::accept_authenticated(s, identity),
        private::connect(c, &invite)
    );
    drop(v.unwrap());
    h.unwrap().1
}
async fn send(t: &mut dyn Transport, m: Message) {
    let (k, p) = m.to_frame_parts().unwrap();
    t.send(k, &p).await.unwrap();
}
async fn read(t: &mut dyn Transport) -> Message {
    let f = tokio::time::timeout(Duration::from_secs(3), t.recv())
        .await
        .unwrap()
        .unwrap();
    Message::from_frame_parts(f.kind, &f.payload).unwrap()
}
fn launch(
    t: Box<dyn Transport>,
    peer: AuthenticatedPeer,
    registry: ResumeRegistry,
) -> tokio::task::JoinHandle<Result<(), Error>> {
    tokio::spawn(serve_authenticated(
        t,
        Pipeline::new(Source, Encode, 30).unwrap(),
        SessionConfig {
            session_id: 0,
            codec: Codec::Raw,
            width: 2,
            height: 2,
            fps: 30,
            write_timeout: Duration::from_secs(1),
            handshake_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(5),
        },
        None,
        peer,
        registry,
    ))
}
async fn private_connection(
    identity: &ShareIdentity,
) -> (Box<dyn Transport>, Box<dyn Transport>, AuthenticatedPeer) {
    let (c, s) = sockets().await;
    let invite = identity
        .invitation("127.0.0.1:5000".parse().unwrap())
        .unwrap();
    let (h, v) = tokio::join!(
        private::accept_authenticated(s, identity),
        private::connect(c, &invite)
    );
    let (h, p) = h.unwrap();
    (
        Box::new(FramedTransport::from_stream(h)),
        Box::new(FramedTransport::from_stream(v.unwrap())),
        p,
    )
}
async fn reject(
    t: Box<dyn Transport>,
    mut v: Box<dyn Transport>,
    p: AuthenticatedPeer,
    r: ResumeRegistry,
    id: u64,
    last_seq: u64,
) {
    let task = launch(t, p, r);
    send(
        v.as_mut(),
        Message::Resume {
            session_id: id,
            last_seq,
        },
    )
    .await;
    assert!(matches!(
        read(v.as_mut()).await,
        Message::HelloAck {
            session_id: 0,
            accepted: false
        }
    ));
    assert!(task.await.unwrap().is_err());
    assert!(v.recv().await.is_err());
}
#[tokio::test(start_paused = true)]
async fn registry_bounds_expiry_private_principals_and_sequence_validation() {
    let first = ShareIdentity::generate().unwrap();
    let second = ShareIdentity::generate().unwrap();
    let a = principal(&first).await;
    let same = principal(&first).await;
    let b = principal(&second).await;
    let registry = ResumeRegistry::default();
    let id = registry.create(&a).unwrap();
    assert!(registry.resume(id, &b, 0).is_none());
    assert!(registry.resume(id, &same, 1).is_none());
    registry.sent(id, 7);
    assert_eq!(registry.resume(id, &same, 7), Some(7));
    assert!(registry.resume(id, &same, u64::MAX).is_none());
    assert!(registry.resume(id ^ 1, &same, 0).is_none());
    for _ in 0..16 {
        registry.create(&a).unwrap();
    }
    assert!(registry.resume(id, &a, 0).is_none());
    let id = registry.create(&a).unwrap();
    tokio::time::advance(Duration::from_secs(301)).await;
    assert!(registry.resume(id, &a, 0).is_none());
}
#[tokio::test]
async fn private_link_resume_same_share_only_and_unknown_never_get_video() {
    let share = ShareIdentity::generate().unwrap();
    let other = ShareIdentity::generate().unwrap();
    let registry = ResumeRegistry::default();
    let p = principal(&share).await;
    let id = registry.create(&p).unwrap();
    registry.sent(id, 9);
    let (h, mut v, p) = private_connection(&share).await;
    let task = launch(h, p, registry.clone());
    send(
        v.as_mut(),
        Message::Resume {
            session_id: id,
            last_seq: 8,
        },
    )
    .await;
    assert!(
        matches!(read(v.as_mut()).await,Message::HelloAck {accepted:true,session_id} if session_id==id)
    );
    assert!(matches!(
        read(v.as_mut()).await,
        Message::VideoConfig { .. }
    ));
    assert!(matches!(
        read(v.as_mut()).await,
        Message::VideoFrame {
            seq: 10,
            keyframe: true,
            ..
        }
    ));
    drop(v);
    let _ = task.await.unwrap();
    let (h, v, p) = private_connection(&other).await;
    reject(h, v, p, registry.clone(), id, 0).await;
    let (h, v, p) = private_connection(&share).await;
    reject(h, v, p, registry.clone(), id ^ 1, 0).await;
    let (h, v, p) = private_connection(&share).await;
    reject(h, v, p, registry, id, u64::MAX).await;
}
#[tokio::test]
async fn paired_resume_same_peer_different_peer_and_forget_after_authentication() {
    let dir = Directory::new();
    let hd = dir.0.join("host");
    let ad = dir.0.join("a");
    let bd = dir.0.join("b");
    let h = DeviceIdentity::open(&hd).unwrap();
    let a = DeviceIdentity::open(&ad).unwrap();
    let b = DeviceIdentity::open(&bd).unwrap();
    let mut peers = Vec::new();
    let mut host_ids = Vec::new();
    for viewer in [&a, &b] {
        let (c, s) = sockets().await;
        let mode = PairingMode::enable();
        let (hp, vp) = tokio::join!(
            paired::pair_host(s, &h, Some(&mode), |_| async { true }),
            paired::pair_client(c, viewer, |_| async { true })
        );
        let hp = hp.unwrap();
        host_ids.push(hp.id());
        DeviceStore::open(&hd).unwrap().insert(hp).unwrap();
        peers.push(vp.unwrap());
    }
    let registry = ResumeRegistry::default();
    let (c, s) = sockets().await;
    let (hp, vp) = tokio::join!(
        paired::accept_authenticated(s, &h, &hd),
        paired::connect(c, &a, &peers[0])
    );
    let (ht, p) = hp.unwrap();
    let task = launch(
        Box::new(FramedTransport::from_stream(ht)),
        p,
        registry.clone(),
    );
    let mut v = FramedTransport::from_stream(vp.unwrap());
    send(
        &mut v,
        Message::Hello {
            proto_version: PROTOCOL_VERSION,
            device_name: "Test".into(),
            role: Role::Viewer,
            capabilities: vec![],
        },
    )
    .await;
    let id = match read(&mut v).await {
        Message::HelloAck {
            accepted: true,
            session_id,
        } => session_id,
        _ => panic!("expected acceptance"),
    };
    read(&mut v).await;
    let seq = match read(&mut v).await {
        Message::VideoFrame { seq, .. } => seq,
        _ => panic!("expected video"),
    };
    drop(v);
    let _ = task.await.unwrap();
    let (c, s) = sockets().await;
    let (hp, vp) = tokio::join!(
        paired::accept_authenticated(s, &h, &hd),
        paired::connect(c, &a, &peers[0])
    );
    let (ht, p) = hp.unwrap();
    let mut v = FramedTransport::from_stream(vp.unwrap());
    let task = launch(
        Box::new(FramedTransport::from_stream(ht)),
        p,
        registry.clone(),
    );
    send(
        &mut v,
        Message::Resume {
            session_id: id,
            last_seq: seq,
        },
    )
    .await;
    assert!(matches!(
        read(&mut v).await,
        Message::HelloAck { accepted: true, .. }
    ));
    read(&mut v).await;
    assert!(matches!(read(&mut v).await,Message::VideoFrame {seq:n,keyframe:true,..} if n>seq));
    // Mid-stream Resume retains the IDR request behavior.
    send(
        &mut v,
        Message::Resume {
            session_id: id,
            last_seq: seq,
        },
    )
    .await;
    let mut idr = false;
    for _ in 0..20 {
        if matches!(
            read(&mut v).await,
            Message::VideoFrame { keyframe: true, .. }
        ) {
            idr = true;
            break;
        }
    }
    assert!(idr);
    drop(v);
    let _ = task.await.unwrap();
    let (c, s) = sockets().await;
    let (hp, vp) = tokio::join!(
        paired::accept_authenticated(s, &h, &hd),
        paired::connect(c, &b, &peers[1])
    );
    let (ht, p) = hp.unwrap();
    reject(
        Box::new(FramedTransport::from_stream(ht)),
        Box::new(FramedTransport::from_stream(vp.unwrap())),
        p,
        registry.clone(),
        id,
        seq,
    )
    .await;
    // Revoke after TLS authorization: Resume must consult the store again.
    let (c, s) = sockets().await;
    let (hp, vp) = tokio::join!(
        paired::accept_authenticated(s, &h, &hd),
        paired::connect(c, &a, &peers[0])
    );
    let (ht, p) = hp.unwrap();
    assert!(DeviceStore::open(&hd)
        .unwrap()
        .forget(&host_ids[0])
        .unwrap());
    reject(
        Box::new(FramedTransport::from_stream(ht)),
        Box::new(FramedTransport::from_stream(vp.unwrap())),
        p,
        registry,
        id,
        seq,
    )
    .await;
    let (c, s) = sockets().await;
    let (hp, vp) = tokio::join!(
        paired::accept_authenticated(s, &h, &hd),
        paired::connect(c, &a, &peers[0])
    );
    assert!(hp.is_err() && vp.is_err());
}
