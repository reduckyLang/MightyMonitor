use mightymonitor_core::{
    encode_stream_frame, BackoffPolicy, ReconnectingSession, ResumeRequest, SplitMix64,
    TcpConnector, TcpTransport, Transport, KIND_RESUME,
};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net::TcpListener};

#[tokio::test]
async fn loopback_server_restart_reconnect() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            for seq in 1u64..=3 {
                stream
                    .write_all(&encode_stream_frame(3, &seq.to_le_bytes()).unwrap())
                    .await
                    .unwrap();
            }
            // End halfway through the next frame to exercise truncated-frame recovery.
            let bytes = encode_stream_frame(3, &4u64.to_le_bytes()).unwrap();
            stream.write_all(&bytes[..9]).await.unwrap();
            drop(stream);
            // Keep the listener: rebinding a port is less portable on Windows.
            let (stream, _) = listener.accept().await.unwrap();
            let mut peer = TcpTransport::new(stream).unwrap();
            let resume = peer.recv().await.unwrap();
            assert_eq!(resume.kind, KIND_RESUME);
            assert_eq!(
                ResumeRequest::from_payload(&resume.payload).unwrap(),
                ResumeRequest {
                    session_id: 77,
                    last_seq: 3
                }
            );
            for seq in 4u64..=6 {
                peer.send(3, &seq.to_le_bytes()).await.unwrap();
            }
        });
        let mut session = ReconnectingSession::new(
            TcpConnector::new(address).with_connect_timeout(Duration::from_secs(1)),
            77,
            BackoffPolicy::default()
                .with_initial(Duration::from_millis(2))
                .with_max(Duration::from_millis(10))
                .with_max_attempts(Some(3)),
            SplitMix64::new(5),
        );
        session.connect_initial().await.unwrap();
        for seq in 1u64..=6 {
            let frame = session.recv().await.unwrap();
            assert_eq!(frame.kind, 3);
            assert_eq!(frame.payload, seq.to_le_bytes());
            session.set_last_seq(seq);
        }
        server.await.unwrap();
    })
    .await
    .unwrap();
}
