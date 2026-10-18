//! Linux X11 / GStreamer host executable.
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
mod linux {
    use codec_gst::encoder::{Encoder as GstEncoder, EncoderConfig, PixelFormat};
    use host_linux::{LinuxCapture, X11Capture};
    use mightymonitor_core::{Bytes, Capture, CodecError, Encoder};
    use mightymonitor_host::{
        pipeline::{HostEncoder, Pipeline},
        session::{serve, SessionConfig},
        Error,
    };
    use std::time::Duration;

    struct RgbaCapture(LinuxCapture<X11Capture>);
    impl Capture for RgbaCapture {
        fn next_frame(&mut self) -> Option<Bytes> {
            let mut bytes = self.0.next_frame()?;
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            Some(bytes)
        }
    }
    struct Codec(GstEncoder);
    impl Encoder for Codec {
        fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError> {
            self.0.encode(raw)
        }
        fn set_bitrate_kbps(&mut self, kbps: u32) -> Result<(), CodecError> {
            Encoder::set_bitrate_kbps(&mut self.0, kbps)
        }
    }
    impl HostEncoder for Codec {
        fn encode_video(
            &mut self,
            raw: &[u8],
            timestamp_us: u64,
            force: bool,
        ) -> Result<(Bytes, bool), Error> {
            if force {
                self.0.force_keyframe()?;
            }
            let frame = self.0.encode_frame(raw, timestamp_us)?;
            Ok((frame.data, frame.is_keyframe))
        }
    }
    pub async fn run() -> Result<(), Error> {
        let mut bind = "127.0.0.1:5000".to_string();
        let mut fps = 30;
        let mut bitrate = 4000;
        let mut idle_timeout_secs = 30;
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            if flag == "--help" {
                println!("mm-host [--bind ADDRESS:PORT] [--fps 1..240] [--bitrate KBPS] [--idle-timeout-secs SECONDS]");
                return Ok(());
            }
            let value = args.next().ok_or("missing flag value")?;
            match flag.as_str() {
                "--bind" => bind = value,
                "--fps" => fps = value.parse()?,
                "--bitrate" => bitrate = value.parse()?,
                "--idle-timeout-secs" => idle_timeout_secs = value.parse()?,
                _ => return Err(format!("unknown flag: {flag}").into()),
            }
        }
        if idle_timeout_secs == 0
            || !(1..=240).contains(&fps)
            || !(1..=1_000_000).contains(&bitrate)
        {
            return Err("invalid fps, bitrate or idle timeout".into());
        }
        let listener = tokio::net::TcpListener::bind(&bind).await?;
        if !listener.local_addr()?.ip().is_loopback() {
            eprintln!(
                "WARNING: non-loopback bind exposes streaming without authentication or encryption"
            );
        }
        eprintln!("mm-host listening on {}", listener.local_addr()?);
        // One logical host session survives sequential viewer reconnections.
        let session_id = 1;
        loop {
            let (stream, _) = listener.accept().await?;
            let source = X11Capture::open(None)?;
            let (width, height) = (u32::from(source.width()), u32::from(source.height()));
            let encoder = Codec(GstEncoder::new(EncoderConfig {
                width,
                height,
                fps,
                bitrate_kbps: bitrate,
                format: PixelFormat::Rgba,
                ..Default::default()
            })?);
            let pipeline = Pipeline::new(RgbaCapture(LinuxCapture::new(source)), encoder, fps)?
                .with_bitrate(bitrate)?;
            let config = SessionConfig {
                session_id,
                codec: mightymonitor_core::Codec::H264,
                width,
                height,
                fps,
                write_timeout: Duration::from_secs(2),
                handshake_timeout: Duration::from_secs(5),
                idle_timeout: Duration::from_secs(idle_timeout_secs),
            };
            if let Err(error) = serve(stream, pipeline, config).await {
                eprintln!("viewer session ended: {error}");
            }
        }
    }
}
#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), mightymonitor_host::Error> {
    linux::run().await
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("mm-host capture currently requires Linux with X11 and GStreamer");
}
