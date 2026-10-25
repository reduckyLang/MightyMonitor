use mightymonitor_core::*;
use mightymonitor_viewer::net::{State, ViewerSession};
use std::time::Duration;
use tokio::net::TcpListener;
struct Dummy;
impl Decoder for Dummy {
    fn decode(&mut self, _: &[u8]) -> Result<Vec<u8>, CodecError> {
        Ok(vec![128; 6])
    }
}
async fn send(t: &mut TcpTransport, m: Message) {
    let (k, p) = m.to_frame_parts().unwrap();
    t.send(k, &p).await.unwrap();
}
async fn recv(t: &mut TcpTransport) -> Message {
    let f = tokio::time::timeout(Duration::from_secs(5), t.recv())
        .await
        .unwrap()
        .unwrap();
    Message::from_frame_parts(f.kind, &f.payload).unwrap()
}
async fn config(t: &mut TcpTransport) {
    send(
        t,
        Message::HelloAck {
            session_id: 42,
            accepted: true,
        },
    )
    .await;
    send(
        t,
        Message::VideoConfig {
            codec: Codec::H264,
            width: 2,
            height: 2,
            fps: 30,
        },
    )
    .await;
}
#[tokio::test]
async fn reconnects_after_drop_and_requests_keyframe() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let viewer = ViewerSession::new(Dummy);
    let out = viewer.output.clone();
    let task = tokio::spawn(viewer.run(listener.local_addr().unwrap().to_string()));
    let (s, _) = listener.accept().await.unwrap();
    let mut t = TcpTransport::new(s).unwrap();
    assert!(matches!(
        recv(&mut t).await,
        Message::Hello {
            proto_version: PROTOCOL_VERSION,
            role: Role::Viewer,
            ..
        }
    ));
    config(&mut t).await;
    send(
        &mut t,
        Message::VideoFrame {
            seq: 7,
            timestamp_us: 0,
            keyframe: true,
            data: vec![1],
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(out.lock().unwrap().state, State::Connected);
    out.lock().unwrap().picture = None;
    drop(t);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(out.lock().unwrap().state, State::Reconnecting);
    let (s, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut t = TcpTransport::new(s).unwrap();
    assert_eq!(
        recv(&mut t).await,
        Message::Resume {
            session_id: 42,
            last_seq: 7
        }
    );
    config(&mut t).await;
    send(
        &mut t,
        Message::VideoFrame {
            seq: 8,
            timestamp_us: 0,
            keyframe: false,
            data: vec![2],
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(out.lock().unwrap().picture.is_none());
    send(
        &mut t,
        Message::VideoFrame {
            seq: 9,
            timestamp_us: 0,
            keyframe: true,
            data: vec![2],
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(out.lock().unwrap().picture.is_some());
    send(
        &mut t,
        Message::Bye {
            reason: "done".into(),
        },
    )
    .await;
    task.await.unwrap();
    assert_eq!(out.lock().unwrap().state, State::Closed);
}
#[tokio::test]
async fn reports_netstats_every_second() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let viewer = ViewerSession::new(Dummy);
    let task = tokio::spawn(viewer.run(listener.local_addr().unwrap().to_string()));
    let (s, _) = listener.accept().await.unwrap();
    let mut t = TcpTransport::new(s).unwrap();
    recv(&mut t).await;
    config(&mut t).await;
    send(
        &mut t,
        Message::VideoFrame {
            seq: 1,
            timestamp_us: 0,
            keyframe: true,
            data: vec![1; 1000],
        },
    )
    .await;
    let start = tokio::time::Instant::now();
    for index in 1..=3 {
        let Message::NetStats(stats) = recv(&mut t).await else {
            panic!("expected stats")
        };
        assert_eq!(stats.loss_pct, 0.0);
        if index == 1 {
            assert!((6..=8).contains(&stats.recv_kbps));
        }
        assert!(start.elapsed() >= Duration::from_millis(index * 900));
        assert!(start.elapsed() < Duration::from_millis(index * 1300));
        let Message::Ping { nonce, t_send_us } = recv(&mut t).await else {
            panic!("expected ping")
        };
        send(
            &mut t,
            Message::Pong {
                nonce,
                t_send_us,
                t_peer_recv_us: t_send_us,
                t_peer_send_us: t_send_us,
            },
        )
        .await;
    }
    task.abort();
}

#[tokio::test]
async fn invalid_config_closes_with_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let viewer = ViewerSession::new(Dummy);
    let out = viewer.output.clone();
    let task = tokio::spawn(viewer.run(listener.local_addr().unwrap().to_string()));
    let (s, _) = listener.accept().await.unwrap();
    let mut t = TcpTransport::new(s).unwrap();
    recv(&mut t).await;
    send(
        &mut t,
        Message::HelloAck {
            session_id: 42,
            accepted: true,
        },
    )
    .await;
    send(
        &mut t,
        Message::VideoConfig {
            codec: Codec::Vp9,
            width: 2,
            height: 2,
            fps: 30,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    let out = out.lock().unwrap();
    assert_eq!(out.state, State::Closed);
    assert!(out.error.as_ref().unwrap().contains("unsupported"));
}

#[test]
fn converts_i420_black_and_white_and_rejects_bad_layout() {
    use mightymonitor_viewer::{net::Picture, render::rgb};
    let mut p = Picture {
        i420: vec![16, 235, 16, 235, 128, 128],
        width: 2,
        height: 2,
        decoded: std::time::Instant::now(),
    };
    let converted = rgb(&p).unwrap();
    assert_eq!(&converted[..3], &[0, 0, 0]);
    assert!(converted[3..6].iter().all(|v| *v >= 254));
    p.i420.pop();
    assert!(rgb(&p).is_none());
}
