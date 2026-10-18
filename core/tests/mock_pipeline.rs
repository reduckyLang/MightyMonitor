#![cfg(feature = "mock")]

use std::time::Duration;

use mightymonitor_core::{
    mock::{MockCapture, MockDecoder, MockEncoder, MockInputInjector},
    Capture, Decoder, Encoder, InputInjector, Message, TcpTransport, Transport,
};
use tokio::{net::TcpListener, time::timeout};

#[tokio::test]
async fn mock_pipeline_capture_encode_send_decode() {
    timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut sender = TcpTransport::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let mut receiver = TcpTransport::new(stream).unwrap();
        let mut encoder = MockEncoder::default();
        let mut decoder = MockDecoder;

        for frame_len in [0, 1, 1024] {
            let mut capture = MockCapture::new(3, frame_len);
            for seq in 0..3 {
                let raw = capture.next_frame().unwrap();
                assert_eq!(raw.len(), frame_len);
                if let Some(first) = raw.first() {
                    assert_eq!(*first, seq as u8);
                }
                encoder.set_bitrate_kbps(4000 + seq as u32).unwrap();
                assert_eq!(encoder.bitrate_kbps(), 4000 + seq as u32);
                let message = Message::VideoFrame {
                    seq,
                    timestamp_us: seq * 16667,
                    keyframe: seq == 0,
                    data: encoder.encode(&raw).unwrap(),
                };
                let (kind, payload) = message.to_frame_parts().unwrap();
                sender.send(kind, &payload).await.unwrap();
                let frame = receiver.recv().await.unwrap();
                let received = Message::from_frame_parts(frame.kind, &frame.payload).unwrap();
                assert_eq!(received, message);
                match received {
                    Message::VideoFrame { data, .. } => {
                        assert_eq!(decoder.decode(&data).unwrap(), raw)
                    }
                    other => panic!("expected VideoFrame, got {other:?}"),
                }
            }
            assert!(capture.next_frame().is_none());
        }

        let mut injector = MockInputInjector::default();
        let mut event = vec![1, 2, 3];
        injector.inject(&event);
        event[0] = 9;
        injector.inject(&event);
        assert_eq!(injector.events(), &[vec![1, 2, 3], vec![9, 2, 3]]);
    })
    .await
    .expect("mock pipeline timed out");
}
