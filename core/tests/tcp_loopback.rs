use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use trinix_core::{
    encode_stream_frame, FrameError, FrameHeader, TcpTransport, Transport, TransportError,
    MAX_PAYLOAD_LEN, PROTOCOL_VERSION,
};

const LIMIT: Duration = Duration::from_secs(12);

async fn pair() -> (TcpTransport, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (host, peer) = tokio::join!(
        TcpTransport::connect(listener.local_addr().unwrap()),
        listener.accept()
    );
    (host.unwrap(), peer.unwrap().0)
}

#[tokio::test]
async fn hello_handshake_over_loopback() {
    timeout(LIMIT, async {
        const HELLO: u8 = 1;
        const HELLO_ACK: u8 = 2;
        let (host, peer) = pair().await;
        // Exercise object safety and the Send futures required by task spawning.
        let mut host: Box<dyn Transport> = Box::new(host);
        let viewer = tokio::spawn(async move {
            let mut viewer = TcpTransport::new(peer).unwrap();
            let hello = viewer.recv().await.unwrap();
            assert_eq!(hello.kind, HELLO);
            assert_eq!(hello.payload, b"hello viewer");
            viewer.send(HELLO_ACK, b"hello host").await.unwrap();
        });
        host.send(HELLO, b"hello viewer").await.unwrap();
        let ack = host.recv().await.unwrap();
        assert_eq!(ack.kind, HELLO_ACK);
        assert_eq!(ack.payload, b"hello host");
        viewer.await.unwrap();
    })
    .await
    .unwrap();
}

fn video_payload(seq: u64) -> Vec<u8> {
    let size = if seq == 500 {
        2 * 1024 * 1024
    } else {
        8 + (seq as usize * 37 % 4096)
    };
    let mut payload: Vec<_> = (0..size).map(|i| (i as u64 ^ seq) as u8).collect();
    payload[..8].copy_from_slice(&seq.to_le_bytes());
    payload
}

#[tokio::test]
async fn streams_1000_video_frames_in_order() {
    timeout(LIMIT, async {
        const VIDEO: u8 = 3;
        let (mut host, peer) = pair().await;
        let sender = tokio::spawn(async move {
            for seq in 0..1000 {
                host.send(VIDEO, &video_payload(seq)).await.unwrap();
            }
        });
        let mut viewer = TcpTransport::new(peer).unwrap();
        for seq in 0..1000 {
            let frame = viewer.recv().await.unwrap();
            assert_eq!(frame.kind, VIDEO);
            assert_eq!(
                u64::from_le_bytes(frame.payload[..8].try_into().unwrap()),
                seq
            );
            assert_eq!(frame.payload, video_payload(seq));
        }
        sender.await.unwrap();
        assert!(matches!(viewer.recv().await, Err(TransportError::Closed)));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn ping_pong_roundtrip() {
    timeout(LIMIT, async {
        const PING: u8 = 4;
        const PONG: u8 = 5;
        let (mut host, peer) = pair().await;
        let viewer = tokio::spawn(async move {
            let mut viewer = TcpTransport::new(peer).unwrap();
            let ping = viewer.recv().await.unwrap();
            assert_eq!(ping.kind, PING);
            viewer.send(PONG, &ping.payload).await.unwrap();
        });
        let nonce = 0x123456789abcdef0u64.to_le_bytes();
        host.send(PING, &nonce).await.unwrap();
        let pong = host.recv().await.unwrap();
        assert_eq!(pong.kind, PONG);
        assert_eq!(pong.payload, nonce);
        viewer.await.unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn peer_close_returns_clean_error() {
    timeout(LIMIT, async {
        let (mut host, peer) = pair().await;
        drop(peer);
        assert!(matches!(host.recv().await, Err(TransportError::Closed)));
        let bytes = encode_stream_frame(1, b"partial payload").unwrap();
        // Cover both a partial header and a partial payload.
        for count in [3, bytes.len() - 1] {
            let (mut host, mut peer) = pair().await;
            peer.write_all(&bytes[..count]).await.unwrap();
            drop(peer);
            assert!(matches!(
                host.recv().await,
                Err(TransportError::Frame(FrameError::Truncated))
            ));
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn garbage_bytes_rejected() {
    timeout(LIMIT, async {
        for (header, expected) in [
            ([0xff, 0, 0, 0, 0, 0], FrameError::UnsupportedVersion(0xff)),
            (
                FrameHeader {
                    version: PROTOCOL_VERSION,
                    kind: 3,
                    payload_len: MAX_PAYLOAD_LEN as u32 + 1,
                }
                .encode(),
                FrameError::TooLarge {
                    len: MAX_PAYLOAD_LEN as u64 + 1,
                    max: MAX_PAYLOAD_LEN as u64,
                },
            ),
        ] {
            let (mut host, mut peer) = pair().await;
            peer.write_all(&header).await.unwrap();
            // Keep the peer open: errors must arrive without waiting for EOF.
            for _ in 0..2 {
                match host.recv().await {
                    Err(TransportError::Frame(error)) => assert_eq!(error, expected),
                    other => panic!("expected frame error, got {other:?}"),
                }
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn read_timeout_is_reported_and_can_be_retried() {
    timeout(LIMIT, async {
        let (mut host, mut peer) = pair().await;
        host.set_read_timeout(Some(Duration::from_millis(25)));
        let start = Instant::now();
        assert!(matches!(
            host.recv().await,
            Err(TransportError::Timeout("read"))
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        // A timeout during a partial frame preserves decoder state.
        let bytes = encode_stream_frame(9, b"retry").unwrap();
        peer.write_all(&bytes[..7]).await.unwrap();
        assert!(matches!(
            host.recv().await,
            Err(TransportError::Timeout("read"))
        ));
        host.set_read_timeout(None);
        peer.write_all(&bytes[7..]).await.unwrap();
        let frame = host.recv().await.unwrap();
        assert_eq!(frame.kind, 9);
        assert_eq!(frame.payload, b"retry");
    })
    .await
    .unwrap();
}
