//! Accepted TCP sessions using the core reader/writer split without frame replay.
use crate::{
    pipeline::{HostEncoder, Pipeline},
    Error,
};
use async_trait::async_trait;
use mightymonitor_core::{
    BackoffPolicy, Codec, Connector, Message, ReconnectingSession, Role, SendOutcome,
    SessionWriter, SplitMix64, TcpTransport, Transport, TransportError, PROTOCOL_VERSION,
};
use std::time::Duration;
use tokio::{net::TcpStream, sync::mpsc};

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub session_id: u64,
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub write_timeout: Duration,
}
/// A server accepts a replacement socket in its listener, rather than dialing
/// the viewer. Disable core retries and never replay old video on replacement.
struct Accepted(Option<TcpTransport>);
#[async_trait]
impl Connector for Accepted {
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
        self.0
            .take()
            .map(|t| Box::new(t) as Box<dyn Transport>)
            .ok_or(TransportError::Closed)
    }
}
struct ReaderTask(tokio::task::JoinHandle<()>);
impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn send(
    writer: &mut SessionWriter,
    message: Message,
    timeout: Duration,
) -> Result<(), Error> {
    let (kind, payload) = message.to_frame_parts()?;
    match writer.send(kind, &payload, timeout).await? {
        SendOutcome::Sent => Ok(()),
        _ => Err("viewer disconnected; accept a new connection".into()),
    }
}

/// Viewer sends Hello first (or Resume with this logical session id). Host
/// replies HelloAck then VideoConfig. Resume also requests an IDR mid-stream.
/// Sequence numbers continue after last_seq on resume; old video is not replayed.
pub async fn serve<E: HostEncoder>(
    stream: TcpStream,
    mut pipeline: Pipeline<E>,
    config: SessionConfig,
) -> Result<(), Error> {
    let transport = TcpTransport::new(stream)?;
    let mut session = ReconnectingSession::new(
        Accepted(Some(transport)),
        config.session_id,
        BackoffPolicy::default().with_max_attempts(Some(0)),
        SplitMix64::new(config.session_id),
    );
    session.connect_initial().await?;
    let (mut reader, mut writer) = session.split();
    let (tx, mut controls) = mpsc::channel(8);
    let _reader_task = ReaderTask(tokio::spawn(async move {
        while let Ok(frame) = reader.recv().await {
            let Ok(message) = Message::from_frame_parts(frame.kind, &frame.payload) else {
                break;
            };
            // A flooding viewer cannot allocate unbounded controls or stall the
            // reader command loop. Closing makes pending writes fail promptly.
            if tx.try_send(message).is_err() {
                break;
            }
        }
    }));
    let first = tokio::time::timeout(Duration::from_secs(5), controls.recv())
        .await?
        .ok_or("viewer closed during handshake")?;
    let mut seq = 0;
    let accepted = match first {
        Message::Hello {
            proto_version,
            role: Role::Viewer,
            ..
        } => proto_version == PROTOCOL_VERSION,
        Message::Resume {
            session_id,
            last_seq,
        } if session_id == config.session_id => {
            seq = last_seq;
            true
        }
        _ => false,
    };
    send(
        &mut writer,
        Message::HelloAck {
            session_id: config.session_id,
            accepted,
        },
        config.write_timeout,
    )
    .await?;
    if !accepted {
        return Err("invalid viewer handshake".into());
    }
    send(
        &mut writer,
        Message::VideoConfig {
            codec: config.codec,
            width: config.width,
            height: config.height,
            fps: config.fps,
        },
        config.write_timeout,
    )
    .await?;
    let mut force = true;
    loop {
        while let Ok(message) = controls.try_recv() {
            match message {
                Message::Ping { nonce, t_send_us } => {
                    send(
                        &mut writer,
                        Message::Pong { nonce, t_send_us },
                        config.write_timeout,
                    )
                    .await?
                }
                Message::Resume { session_id, .. } if session_id == config.session_id => {
                    force = true
                }
                Message::NetStats(stats) => pipeline.on_stats(&stats)?,
                Message::Bye { .. } => return Ok(()),
                Message::Pong { .. } => {}
                _ => return Err("unexpected viewer message".into()),
            }
        }
        if controls.is_closed() {
            return Err("viewer disconnected".into());
        }
        let Some(video) = pipeline.next(force).await? else {
            return Ok(());
        };
        force = false;
        seq = seq.checked_add(1).ok_or("sequence exhausted")?;
        let captured = video.captured;
        let (kind, payload) = Message::VideoFrame {
            seq,
            timestamp_us: video.timestamp_us,
            keyframe: video.keyframe,
            data: video.data,
        }
        .to_frame_parts()?;
        // Serialization is included in the freshness budget too. If an encoded
        // reference is discarded, request an IDR before any further video.
        if !pipeline.sending(captured) {
            force = true;
            continue;
        }
        if writer.send(kind, &payload, config.write_timeout).await? != SendOutcome::Sent {
            return Err("viewer disconnected".into());
        }
        pipeline.sent();
    }
}
