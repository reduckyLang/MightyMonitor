#![forbid(unsafe_code)]
use mightymonitor_core::{
    Bytes, Capture, Codec, CodecError, Encoder, Message, Role, TcpTransport, Transport,
    PROTOCOL_VERSION,
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
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError> {
        Ok(raw.to_vec())
    }
    fn set_bitrate_kbps(&mut self, _: u32) -> Result<(), CodecError> {
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
    start_with_idle(size, resume, Duration::from_secs(30)).await
}
async fn start_with_idle(
    size: usize,
    resume: bool,
    idle_timeout: Duration,
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
                handshake_timeout: Duration::from_secs(5),
                idle_timeout,
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
    // Withhold reads until backpressure causes a drop, allowing for different
    // TCP buffer sizes and CI scheduling instead of assuming a fixed delay.
    let drop_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while monitor.snapshot().dropped == 0 {
        assert!(
            tokio::time::Instant::now() < drop_deadline,
            "no frames dropped under backpressure: {:?}",
            monitor.snapshot()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Buffered frames can precede the first gap. Look further than three frames,
    // but accept the host's drop counter if the gap is still buffered at the bound.
    let read_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut captured = Vec::new();
    let mut saw_gap = false;
    for _ in 0..20 {
        let Ok(message) = tokio::time::timeout_at(read_deadline, recv(&mut viewer)).await else {
            break;
        };
        if let Message::VideoFrame { data, .. } = message {
            let capture = u64::from_le_bytes(data[..8].try_into().unwrap());
            if let Some(&previous) = captured.last() {
                assert!(
                    capture > previous,
                    "capture counters: {captured:?}, {capture}"
                );
                saw_gap |= capture > previous + 1;
            }
            captured.push(capture);
            if saw_gap {
                break;
            }
        }
    }
    let metrics = monitor.snapshot();
    assert!(captured.len() >= 2, "stream did not resume: {metrics:?}");
    assert!(saw_gap || metrics.dropped > 0, "{metrics:?}");
    assert!(metrics.queue_len <= 1, "{metrics:?}");
    // Slot::push always records one, and receiving frames proves it ran.
    assert_eq!(metrics.max_queue_len, 1);
    // Age measures capture to send initiation, excluding peer-controlled TCP
    // delivery. Allow scheduling slack while keeping it well below write timeout.
    assert!(
        metrics.max_send_age < Duration::from_millis(500),
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

#[tokio::test]
async fn rejects_invalid_and_missing_handshakes() {
    for first in [
        Some(Message::Hello {
            proto_version: PROTOCOL_VERSION + 1,
            device_name: "bad version".into(),
            role: Role::Viewer,
            capabilities: vec![],
        }),
        Some(Message::Hello {
            proto_version: PROTOCOL_VERSION,
            device_name: "wrong role".into(),
            role: Role::Host,
            capabilities: vec![],
        }),
        None,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let host = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            serve(
                stream,
                Pipeline::new(FakeCapture(0), DummyEncoder { size: 8 }, 30)?,
                SessionConfig {
                    session_id: 42,
                    codec: Codec::Raw,
                    width: 1,
                    height: 1,
                    fps: 30,
                    write_timeout: Duration::from_secs(1),
                    handshake_timeout: Duration::from_millis(100),
                    idle_timeout: Duration::from_secs(30),
                },
            )
            .await
        });
        let mut viewer = TcpTransport::connect(address).await.unwrap();
        if let Some(message) = first {
            send(&mut viewer, message).await;
            assert!(matches!(
                recv(&mut viewer).await,
                Message::HelloAck {
                    accepted: false,
                    ..
                }
            ));
        }
        assert!(tokio::time::timeout(Duration::from_secs(2), host)
            .await
            .unwrap()
            .unwrap()
            .is_err());
    }
}

#[tokio::test]
async fn capture_panic_closes_pipeline() {
    struct PanickingCapture;
    impl Capture for PanickingCapture {
        fn next_frame(&mut self) -> Option<Bytes> {
            panic!("backend failed")
        }
    }
    let mut pipeline = Pipeline::new(PanickingCapture, DummyEncoder { size: 8 }, 30).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), pipeline.next(false))
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn silent_viewer_times_out_but_pings_reset_idle() {
    let (mut viewer, _, mut host) = start_with_idle(8, false, Duration::from_millis(500)).await;
    for nonce in 0..4 {
        send(
            &mut viewer,
            Message::Ping {
                nonce,
                t_send_us: 0,
            },
        )
        .await;
        loop {
            if matches!(recv(&mut viewer).await, Message::Pong { .. }) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!host.is_finished());
    }
    // Continue draining video so a blocked TCP writer cannot explain termination.
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            tokio::select! {
                result = &mut host => break result.unwrap(),
                _ = viewer.recv() => {}
            }
        }
    })
    .await
    .unwrap();
    assert!(result.unwrap_err().to_string().contains("idle timeout"));
}
