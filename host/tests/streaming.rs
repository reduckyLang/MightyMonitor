#![forbid(unsafe_code)]
use mightymonitor_core::{
    Bytes, Capture, Codec, Encoder, Message, Role, TcpTransport, Transport, PROTOCOL_VERSION,
};
use mightymonitor_host::{
    pipeline::{HostEncoder, Monitor, Pipeline},
    session::{serve, SessionConfig},
    Error,
};
use std::time::Duration;
use tokio::net::TcpListener;

struct FakeCapture(u64);
impl Capture for FakeCapture {
    fn next_frame(&mut self) -> Option<Bytes> {
        self.0 += 1;
        Some(self.0.to_le_bytes().to_vec())
    }
}
struct DummyEncoder {
    size: usize,
}
impl Encoder for DummyEncoder {
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, mightymonitor_core::CodecError> {
        Ok(raw.to_vec())
    }
    fn set_bitrate_kbps(&mut self, _: u32) -> Result<(), mightymonitor_core::CodecError> {
        Ok(())
    }
}
impl HostEncoder for DummyEncoder {
    fn encode_video(&mut self, raw: &[u8], _: u64, force: bool) -> Result<(Bytes, bool), Error> {
        let mut data = vec![0; self.size.max(8)];
        data[..8].copy_from_slice(raw);
        Ok((data, force))
    }
}
async fn send(t: &mut TcpTransport, message: Message) {
    let (kind, data) = message.to_frame_parts().unwrap();
    t.send(kind, &data).await.unwrap();
}
async fn recv(t: &mut TcpTransport) -> Message {
    let frame = tokio::time::timeout(Duration::from_secs(5), t.recv())
        .await
        .unwrap()
        .unwrap();
    Message::from_frame_parts(frame.kind, &frame.payload).unwrap()
}
async fn start(
    size: usize,
    resume: bool,
) -> (
    TcpTransport,
    Monitor,
    tokio::task::JoinHandle<Result<(), Error>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pipeline = Pipeline::new(
        FakeCapture(0),
        DummyEncoder { size },
        if size > 8 { 10 } else { 30 },
    )
    .unwrap();
    let monitor = pipeline.monitor();
    let address = listener.local_addr().unwrap();
    let host = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        serve(
            stream,
            pipeline,
            SessionConfig {
                session_id: 42,
                codec: Codec::Raw,
                width: 1,
                height: 1,
                fps: if size > 8 { 10 } else { 30 },
                write_timeout: Duration::from_secs(3),
            },
        )
        .await
    });
    let mut viewer = TcpTransport::connect(address).await.unwrap();
    let hello = if resume {
        Message::Resume {
            session_id: 42,
            last_seq: 100,
        }
    } else {
        Message::Hello {
            proto_version: PROTOCOL_VERSION,
            device_name: "test".into(),
            role: Role::Viewer,
            capabilities: vec![],
        }
    };
    send(&mut viewer, hello).await;
    assert!(matches!(
        recv(&mut viewer).await,
        Message::HelloAck {
            session_id: 42,
            accepted: true
        }
    ));
    assert!(matches!(
        recv(&mut viewer).await,
        Message::VideoConfig { fps, .. } if fps == if size > 8 { 10 } else { 30 }
    ));
    (viewer, monitor, host)
}
async fn stop(mut viewer: TcpTransport, host: tokio::task::JoinHandle<Result<(), Error>>) {
    send(
        &mut viewer,
        Message::Bye {
            reason: "test complete".into(),
        },
    )
    .await;
    // Closing the socket may race Bye; either path must terminate promptly.
    drop(viewer);
    let _ = tokio::time::timeout(Duration::from_secs(5), host)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn host_streams_n_frames() {
    let (mut viewer, monitor, host) = start(8, false).await;
    let mut previous = 0;
    for n in 0..6 {
        match recv(&mut viewer).await {
            Message::VideoFrame {
                seq,
                keyframe,
                data,
                ..
            } => {
                assert_eq!(seq, n + 1);
                assert_eq!(keyframe, n == 0);
                let capture = u64::from_le_bytes(data.try_into().unwrap());
                assert!(capture > previous);
                previous = capture;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    send(
        &mut viewer,
        Message::NetStats(mightymonitor_core::NetStats {
            rtt_ms: 50,
            loss_pct: 5.0,
            recv_kbps: 1000,
        }),
    )
    .await;
    send(
        &mut viewer,
        Message::Ping {
            nonce: 7,
            t_send_us: 123,
        },
    )
    .await;
    loop {
        if let Message::Pong { nonce, t_send_us } = recv(&mut viewer).await {
            assert_eq!((nonce, t_send_us), (7, 123));
            break;
        }
    }
    assert_eq!(monitor.snapshot().max_queue_len, 1);
    stop(viewer, host).await;
}
#[tokio::test]
async fn slow_viewer_drops_frames_not_latency() {
    let (mut viewer, monitor, host) = start(2 * 1024 * 1024, false).await;
    // A frame exceeds typical TCP buffers: withholding reads stalls the writer.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(monitor.snapshot().dropped > 0);
    let mut captured = Vec::new();
    for _ in 0..3 {
        if let Message::VideoFrame { data, .. } = recv(&mut viewer).await {
            captured.push(u64::from_le_bytes(data[..8].try_into().unwrap()));
        }
    }
    assert!(captured.windows(2).any(|pair| pair[1] > pair[0] + 1));
    let metrics = monitor.snapshot();
    assert!(metrics.queue_len <= 1);
    assert_eq!(metrics.max_queue_len, 1);
    assert!(
        metrics.max_send_age < Duration::from_secs_f64(2.0 / 10.0),
        "{metrics:?}"
    );
    stop(viewer, host).await;
}
#[tokio::test]
async fn keyframe_on_viewer_request() {
    let (mut viewer, _, host) = start(8, false).await;
    assert!(matches!(
        recv(&mut viewer).await,
        Message::VideoFrame { keyframe: true, .. }
    ));
    assert!(matches!(
        recv(&mut viewer).await,
        Message::VideoFrame {
            keyframe: false,
            ..
        }
    ));
    send(
        &mut viewer,
        Message::Resume {
            session_id: 42,
            last_seq: 0,
        },
    )
    .await;
    let mut found = false;
    for _ in 0..5 {
        if matches!(
            recv(&mut viewer).await,
            Message::VideoFrame { keyframe: true, .. }
        ) {
            found = true;
            break;
        }
    }
    assert!(found);
    stop(viewer, host).await;
    let (mut viewer, _, host) = start(8, true).await;
    assert!(matches!(
        recv(&mut viewer).await,
        Message::VideoFrame {
            seq: 101,
            keyframe: true,
            ..
        }
    ));
    stop(viewer, host).await;
}
