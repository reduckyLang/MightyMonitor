//! Real H.264 encoder -> TCP host -> viewer -> real decoder (no GUI or desktop).
#![cfg(any(target_os = "linux", target_os = "windows"))]
use codec_gst::encoder::{Encoder as H264Encoder, EncoderConfig, PixelFormat};
use std::time::Duration;
use tokio::net::TcpListener;
use trinix_core::{Bytes, Capture, Codec, CodecError, Encoder};
use trinix_host::{
    pipeline::{HostEncoder, Pipeline},
    session::{serve_transport, SessionConfig},
    Error,
};
struct SolidFrame;
impl Capture for SolidFrame {
    fn next_frame(&mut self) -> Option<Bytes> {
        Some([180, 80, 40, 255].repeat(64 * 64))
    }
}
struct VideoEncoder(H264Encoder);
impl Encoder for VideoEncoder {
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError> {
        self.0.encode(raw)
    }
    fn set_bitrate_kbps(&mut self, bitrate: u32) -> Result<(), CodecError> {
        Encoder::set_bitrate_kbps(&mut self.0, bitrate)
    }
}
impl HostEncoder for VideoEncoder {
    fn encode_video(
        &mut self,
        raw: &[u8],
        timestamp: u64,
        force: bool,
    ) -> Result<(Bytes, bool), Error> {
        if force {
            self.0.force_keyframe()?;
        }
        let frame = self.0.encode_frame(raw, timestamp)?;
        Ok((frame.data, frame.is_keyframe))
    }
}
#[tokio::test]
async fn real_h264_reaches_viewer_over_tcp() {
    exercise(false).await;
}
#[tokio::test]
async fn real_h264_reaches_viewer_over_authenticated_tls() {
    exercise(true).await;
}
async fn exercise(secure: bool) {
    let encoder = H264Encoder::new(EncoderConfig {
        width: 64,
        height: 64,
        fps: 10,
        bitrate_kbps: 1000,
        format: PixelFormat::Rgba,
        ..Default::default()
    });
    let encoder = match encoder {
        Ok(encoder) => encoder,
        Err(error) => {
            assert_ne!(
                std::env::var("MM_REQUIRE_CODECS").as_deref(),
                Ok("1"),
                "Required encoder unavailable: {error}"
            );
            eprintln!("SKIP real H.264 transport: {error}");
            return;
        }
    };
    let decoder = codec_gst::decoder::Decoder::new().expect("H.264 decoder required");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let identity = secure.then(|| trinix_security::ShareIdentity::generate().unwrap());
    let address = identity.as_ref().map_or_else(
        || listener.local_addr().unwrap().to_string(),
        |id| {
            id.invitation(listener.local_addr().unwrap())
                .unwrap()
                .to_link()
        },
    );
    let host = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let transport: Box<dyn trinix_core::Transport> = if let Some(identity) = identity {
            Box::new(trinix_core::FramedTransport::from_stream(
                trinix_security::asynchronous::accept(stream, &identity)
                    .await
                    .unwrap(),
            ))
        } else {
            Box::new(trinix_core::TcpTransport::new(stream).unwrap())
        };
        let pipeline = Pipeline::new(SolidFrame, VideoEncoder(encoder), 10)
            .unwrap()
            .with_bitrate(1000)
            .unwrap();
        serve_transport(
            transport,
            pipeline,
            SessionConfig {
                session_id: 11,
                codec: Codec::H264,
                width: 64,
                height: 64,
                fps: 10,
                write_timeout: Duration::from_secs(2),
                handshake_timeout: Duration::from_secs(5),
                idle_timeout: Duration::from_secs(5),
            },
        )
        .await
    });
    let session = trinix_viewer::net::ViewerSession::new(decoder);
    let output = session.output.clone();
    let viewer = tokio::spawn(session.run(address));
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            {
                let out = output.lock().unwrap();
                if let Some(error) = &out.error {
                    panic!("viewer failed: {error}");
                }
                if let Some(picture) = &out.picture {
                    assert_eq!(out.state, trinix_viewer::net::State::Connected);
                    assert_eq!((picture.width, picture.height), (64, 64));
                    assert_eq!(picture.i420.len(), 64 * 64 * 3 / 2);
                    assert!(picture.i420[..64 * 64].iter().all(|&luma| luma > 20));
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    viewer.abort();
    host.abort();
    let _ = viewer.await;
    let _ = host.await;
    result.expect("real H.264 did not reach the viewer");
}
