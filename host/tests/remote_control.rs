//! Authorization and disconnect cleanup exercised over real framed sockets.
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use trinix_core::{
    Bytes, Capture, Codec, CodecError, Encoder, InputEvent, Message, Role, TcpTransport, Transport,
    PROTOCOL_VERSION,
};
use trinix_host::{
    input::InputTarget,
    pipeline::{HostEncoder, Pipeline},
    session::{serve_with_input, SessionConfig},
    Error,
};
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
struct Recorder {
    events: Arc<Mutex<Vec<InputEvent>>>,
    released: Arc<AtomicUsize>,
}
impl InputTarget for Recorder {
    fn apply(&mut self, e: &InputEvent) -> Result<(), Error> {
        self.events.lock().unwrap().push(e.clone());
        Ok(())
    }
    fn release_all(&mut self) {
        self.released.fetch_add(1, Ordering::SeqCst);
    }
}
async fn send(t: &mut TcpTransport, m: Message) {
    let (k, p) = m.to_frame_parts().unwrap();
    t.send(k, &p).await.unwrap();
}
async fn read(t: &mut TcpTransport) -> Message {
    let f = tokio::time::timeout(Duration::from_secs(2), t.recv())
        .await
        .unwrap()
        .unwrap();
    Message::from_frame_parts(f.kind, &f.payload).unwrap()
}
async fn until_status(t: &mut TcpTransport, enabled: bool) {
    loop {
        if let Message::ControlStatus { enabled: actual } = read(t).await {
            assert_eq!(actual, enabled);
            return;
        }
    }
}
async fn start(
    allowed: bool,
) -> (
    TcpTransport,
    Arc<Mutex<Vec<InputEvent>>>,
    Arc<AtomicUsize>,
    tokio::task::JoinHandle<Result<(), Error>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let events = Arc::new(Mutex::new(vec![]));
    let released = Arc::new(AtomicUsize::new(0));
    let backend = if allowed {
        Some(Box::new(Recorder {
            events: events.clone(),
            released: released.clone(),
        }) as Box<dyn InputTarget>)
    } else {
        None
    };
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        serve_with_input(
            Box::new(TcpTransport::new(stream)?),
            Pipeline::new(Source, Encode, 30)?,
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
            backend,
        )
        .await
    });
    let mut peer = TcpTransport::connect(address).await.unwrap();
    send(
        &mut peer,
        Message::Hello {
            proto_version: PROTOCOL_VERSION,
            device_name: "test".into(),
            role: Role::Viewer,
            capabilities: vec!["input-v1".into()],
        },
    )
    .await;
    assert!(
        matches!(read(&mut peer).await,Message::HelloAckV2{control_available,..} if control_available==allowed)
    );
    assert!(matches!(read(&mut peer).await, Message::VideoConfig { .. }));
    (peer, events, released, task)
}
#[tokio::test]
async fn view_permission_does_not_authorize_input() {
    let (mut peer, events, _, task) = start(false).await;
    send(&mut peer, Message::ControlRequest { enabled: true }).await;
    until_status(&mut peer, false).await;
    send(
        &mut peer,
        Message::Input(InputEvent::Key {
            code: 4,
            pressed: true,
        }),
    )
    .await;
    assert!(tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert!(events.lock().unwrap().is_empty());
}
#[tokio::test]
async fn input_requires_both_grants_and_releases_on_revoke_disconnect() {
    let (mut peer, events, released, task) = start(true).await;
    send(&mut peer, Message::ControlRequest { enabled: true }).await;
    until_status(&mut peer, true).await;
    send(
        &mut peer,
        Message::Input(InputEvent::Key {
            code: 4,
            pressed: true,
        }),
    )
    .await;
    send(&mut peer, Message::ControlRequest { enabled: false }).await;
    until_status(&mut peer, false).await;
    assert_eq!(
        *events.lock().unwrap(),
        [InputEvent::Key {
            code: 4,
            pressed: true
        }]
    );
    let revocations = released.load(Ordering::SeqCst);
    assert!(revocations >= 2);
    drop(peer);
    let _ = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap();
    assert!(released.load(Ordering::SeqCst) > revocations);
}
#[tokio::test]
async fn malformed_input_closes_the_grant_and_releases_held_state() {
    let (mut peer, events, released, task) = start(true).await;
    send(&mut peer, Message::ControlRequest { enabled: true }).await;
    until_status(&mut peer, true).await;
    send(
        &mut peer,
        Message::Input(InputEvent::MouseMove {
            x: f32::NAN,
            y: 0.0,
        }),
    )
    .await;
    let _ = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap();
    assert!(events.lock().unwrap().is_empty());
    assert!(released.load(Ordering::SeqCst) >= 2);
}
