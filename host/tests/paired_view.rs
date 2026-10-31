//! Paired authentication reaches the video protocol with view authority only.
use trinix_core::{
    Bytes, Capture, Codec, CodecError, Encoder, FramedTransport, Message, Role, Transport,
    PROTOCOL_VERSION,
};
use trinix_host::{
    pipeline::{HostEncoder, Pipeline},
    session::{serve_authenticated, SessionConfig},
    Error,
};
use trinix_security::{
    pairing::{asynchronous as paired, DeviceIdentity, PairingMode},
    store::DeviceStore,
};
use std::{path::PathBuf, time::Duration};
struct Directory(PathBuf);
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (client, server) = tokio::join!(
        tokio::net::TcpStream::connect(listener.local_addr().unwrap()),
        listener.accept()
    );
    (client.unwrap(), server.unwrap().0)
}
async fn send(t: &mut dyn Transport, message: Message) {
    let (k, p) = message.to_frame_parts().unwrap();
    t.send(k, &p).await.unwrap();
}
async fn read(t: &mut dyn Transport) -> Message {
    let frame = tokio::time::timeout(Duration::from_secs(2), t.recv())
        .await
        .unwrap()
        .unwrap();
    Message::from_frame_parts(frame.kind, &frame.payload).unwrap()
}
#[tokio::test]
async fn paired_reconnect_streams_video_and_denies_control() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = Directory(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../target/m8-view-tests/{unique}")),
    );
    let hd = dir.0.join("host");
    let vd = dir.0.join("viewer");
    let host = DeviceIdentity::open(&hd).unwrap();
    let viewer = DeviceIdentity::open(&vd).unwrap();
    let mode = PairingMode::enable();
    let (client, server) = sockets().await;
    let (hpeer, vpeer) = tokio::join!(
        paired::pair_host(server, &host, Some(&mode), |_| async { true }),
        paired::pair_client(client, &viewer, |_| async { true })
    );
    DeviceStore::open(&hd)
        .unwrap()
        .insert(hpeer.unwrap())
        .unwrap();
    let peer = vpeer.unwrap();
    let id = peer.id();
    DeviceStore::open(&vd).unwrap().insert(peer).unwrap();
    drop(host);
    drop(viewer);
    let host = DeviceIdentity::open(&hd).unwrap();
    let viewer = DeviceIdentity::open(&vd).unwrap();
    let peer = DeviceStore::open(&vd).unwrap().find(&id).unwrap();
    let (client, server) = sockets().await;
    let (hchannel, vchannel) = tokio::join!(
        paired::accept_authenticated(server, &host, &hd),
        paired::connect(client, &viewer, &peer)
    );
    let (hchannel, authenticated) = hchannel.unwrap();
    let task = tokio::spawn(async move {
        serve_authenticated(
            Box::new(FramedTransport::from_stream(hchannel)),
            Pipeline::new(Source, Encode, 30).unwrap(),
            SessionConfig {
                session_id: 42,
                codec: Codec::Raw,
                width: 2,
                height: 2,
                fps: 30,
                write_timeout: Duration::from_secs(1),
                handshake_timeout: Duration::from_secs(1),
                idle_timeout: Duration::from_secs(3),
            },
            None,
            authenticated,
            trinix_host::resume::ResumeRegistry::default(),
        )
        .await
    });
    let mut transport = FramedTransport::from_stream(vchannel.unwrap());
    send(
        &mut transport,
        Message::Hello {
            proto_version: PROTOCOL_VERSION,
            device_name: "paired test".into(),
            role: Role::Viewer,
            capabilities: vec!["input-v1".into()],
        },
    )
    .await;
    assert!(matches!(
        read(&mut transport).await,
        Message::HelloAckV2 {
            accepted: true,
            control_available: false,
            ..
        }
    ));
    assert!(matches!(
        read(&mut transport).await,
        Message::VideoConfig { .. }
    ));
    send(&mut transport, Message::ControlRequest { enabled: true }).await;
    let mut video = false;
    let mut denied = false;
    for _ in 0..20 {
        match read(&mut transport).await {
            Message::ControlStatus { enabled } => {
                assert!(!enabled);
                denied = true;
            }
            Message::VideoFrame { .. } => video = true,
            _ => {}
        }
        if video && denied {
            break;
        }
    }
    assert!(video && denied);
    drop(transport);
    let _ = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
}
