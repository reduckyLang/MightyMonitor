//! Accepted TCP sessions using the core reader/writer split without frame replay.
use crate::{
    pipeline::{HostEncoder, Pipeline},
    Error,
};
use async_trait::async_trait;
use mightymonitor_core::{
    BackoffPolicy, Codec, Connector, Message, MonotonicClock, ReconnectingSession, Role,
    SendOutcome, SessionWriter, SplitMix64, TcpTransport, Transport, TransportError,
    PROTOCOL_VERSION,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpStream, sync::mpsc};

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub session_id: u64,
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub write_timeout: Duration,
    pub handshake_timeout: Duration,
    /// Maximum viewer silence after handshake; any valid control resets it.
    pub idle_timeout: Duration,
}
/// A server accepts a replacement socket in its listener, rather than dialing
/// the viewer. Disable core retries and never replay old video on replacement.
struct Accepted(Option<Box<dyn Transport>>);
#[async_trait]
impl Connector for Accepted {
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
        self.0.take().ok_or(TransportError::Closed)
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
    pipeline: Pipeline<E>,
    config: SessionConfig,
) -> Result<(), Error> {
    serve_transport(Box::new(TcpTransport::new(stream)?), pipeline, config).await
}

/// Serve an already established transport. TLS peers must be authenticated
/// before capture/session creation. This does not authorize remote input.
pub async fn serve_transport<E: HostEncoder>(
    transport: Box<dyn Transport>,
    pipeline: Pipeline<E>,
    config: SessionConfig,
) -> Result<(), Error> {
    serve_with_input(transport, pipeline, config, None).await
}
/// The caller must authenticate the peer and obtain local control consent before
/// supplying an input target. Each connection starts without an input grant.
pub async fn serve_with_input<E: HostEncoder>(
    transport: Box<dyn Transport>,
    pipeline: Pipeline<E>,
    config: SessionConfig,
    input: Option<Box<dyn crate::input::InputTarget>>,
) -> Result<(), Error> {
    serve_bound(transport, pipeline, config, input, None).await
}
/// Secure sessions must use the transport-established identity and shared registry.
pub async fn serve_authenticated<E: HostEncoder>(
    transport: Box<dyn Transport>,
    pipeline: Pipeline<E>,
    config: SessionConfig,
    input: Option<Box<dyn crate::input::InputTarget>>,
    peer: mightymonitor_security::AuthenticatedPeer,
    registry: crate::resume::ResumeRegistry,
) -> Result<(), Error> {
    serve_bound(transport, pipeline, config, input, Some((peer, registry))).await
}
async fn serve_bound<E: HostEncoder>(
    transport: Box<dyn Transport>,
    mut pipeline: Pipeline<E>,
    mut config: SessionConfig,
    input: Option<Box<dyn crate::input::InputTarget>>,
    binding: Option<(
        mightymonitor_security::AuthenticatedPeer,
        crate::resume::ResumeRegistry,
    )>,
) -> Result<(), Error> {
    let control_available = input.is_some();
    let input = input.map(crate::input::InputLease);
    let mut session = ReconnectingSession::new(
        Accepted(Some(transport)),
        config.session_id,
        BackoffPolicy::default().with_max_attempts(Some(0)),
        SplitMix64::new(config.session_id),
    );
    session.connect_initial().await?;
    let (mut reader, mut writer) = session.split();
    let (tx, mut controls) = mpsc::channel(128);
    let clock = MonotonicClock::new();
    let reader_clock = clock.clone();
    let activity = Arc::new(Mutex::new(tokio::time::Instant::now()));
    let reader_activity = activity.clone();
    let handshake = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader_handshake = handshake.clone();
    let disconnected = Arc::new(tokio::sync::Notify::new());
    let reader_closed = disconnected.clone();
    let _reader_task = ReaderTask(tokio::spawn(async move {
        struct Closed(Arc<tokio::sync::Notify>);
        impl Drop for Closed {
            fn drop(&mut self) {
                self.0.notify_one();
            }
        }
        let _closed = Closed(reader_closed);
        let mut input = input;
        let mut controlling = false;
        while let Ok(frame) = reader.recv().await {
            let received_at_us = reader_clock.now_us();
            let Ok(message) = Message::from_frame_parts(frame.kind, &frame.payload) else {
                break;
            };
            let ping_received_at =
                matches!(&message, Message::Ping { .. }).then_some(received_at_us);
            let message = match message {
                Message::Input(mightymonitor_core::InputEvent::ReleaseAll) => {
                    if !reader_handshake.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    if let Some(target) = input.as_mut() {
                        target.0.release_all();
                    }
                    continue;
                }
                Message::Input(event) => {
                    if !reader_handshake.load(std::sync::atomic::Ordering::Acquire)
                        || !controlling
                        || !event.validate()
                    {
                        break;
                    }
                    if input
                        .as_mut()
                        .is_none_or(|target| target.0.apply(&event).is_err())
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                    continue;
                }
                Message::ControlRequest { enabled } => {
                    if !reader_handshake.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    if let Some(target) = input.as_mut() {
                        target.0.deactivate();
                    }
                    controlling =
                        enabled && input.as_mut().is_some_and(|target| target.0.activate());
                    Message::ControlStatus {
                        enabled: controlling,
                    }
                }
                Message::ControlStatus { .. } => break,
                other => other,
            };
            *reader_activity.lock().unwrap_or_else(|e| e.into_inner()) =
                tokio::time::Instant::now();
            // A flooding viewer cannot allocate unbounded controls or stall the
            // reader command loop. Closing makes pending writes fail promptly.
            if tx.try_send((message, ping_received_at)).is_err() {
                break;
            }
        }
    }));
    let first = tokio::time::timeout(config.handshake_timeout, controls.recv())
        .await?
        .ok_or("viewer closed during handshake")?;
    let first = first.0;
    let mut seq = 0;
    let supports_input = matches!(&first, Message::Hello {capabilities,..} if capabilities.iter().any(|c| c == "input-v1"));
    let unsupported_version = match &first {
        Message::Hello { proto_version, .. } if *proto_version != PROTOCOL_VERSION => {
            Some(*proto_version)
        }
        _ => None,
    };
    let accepted = match first {
        Message::Hello {
            proto_version,
            role: Role::Viewer,
            ..
        } => {
            if proto_version != PROTOCOL_VERSION {
                false
            } else if let Some((peer, registry)) = &binding {
                if peer.is_current() {
                    config.session_id = registry.create(peer)?;
                    true
                } else {
                    false
                }
            } else {
                true
            }
        }
        Message::Resume {
            session_id,
            last_seq,
        } => {
            if let Some((peer, registry)) = &binding {
                if let Some(sent) = registry.resume(session_id, peer, last_seq) {
                    config.session_id = session_id;
                    seq = sent;
                    true
                } else {
                    false
                }
            } else if session_id == config.session_id {
                seq = last_seq;
                true
            } else {
                false
            }
        }
        _ => false,
    };
    send(
        &mut writer,
        if supports_input {
            Message::HelloAckV2 {
                session_id: if accepted { config.session_id } else { 0 },
                accepted,
                control_available,
            }
        } else {
            Message::HelloAck {
                session_id: if accepted { config.session_id } else { 0 },
                accepted,
            }
        },
        config.write_timeout,
    )
    .await?;
    if !accepted {
        if let Some(version) = unsupported_version {
            return Err(format!(
                "unsupported protocol version {version}; expected {PROTOCOL_VERSION}"
            )
            .into());
        }
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
    handshake.store(true, std::sync::atomic::Ordering::Release);
    let mut force = true;
    *activity.lock().unwrap_or_else(|e| e.into_inner()) = tokio::time::Instant::now();
    loop {
        for _ in 0..64 {
            let Ok((message, ping_received_at)) = controls.try_recv() else {
                break;
            };
            match message {
                Message::Ping { nonce, t_send_us } => {
                    let t_peer_recv_us =
                        ping_received_at.ok_or("missing Ping receive timestamp")?;
                    send(
                        &mut writer,
                        Message::Pong {
                            nonce,
                            t_send_us,
                            t_peer_recv_us,
                            t_peer_send_us: clock.now_us(),
                        },
                        config.write_timeout,
                    )
                    .await?
                }
                Message::KeyframeRequest => force = true,
                Message::Resume {
                    session_id,
                    last_seq,
                } if session_id == config.session_id
                    && (binding.is_none()
                        || (last_seq <= seq
                            && binding.as_ref().is_some_and(|(peer, registry)| {
                                registry.resume(session_id, peer, last_seq).is_some()
                            }))) =>
                {
                    force = true
                }
                Message::Resume { .. } => {
                    send(
                        &mut writer,
                        Message::HelloAck {
                            session_id: 0,
                            accepted: false,
                        },
                        config.write_timeout,
                    )
                    .await?;
                    return Err("invalid viewer handshake".into());
                }
                Message::NetStats(stats) => {
                    // A live bitrate rejection is recoverable: keep streaming at
                    // the previous encoder setting. Initial setup errors are fatal.
                    if let Err(error) = pipeline.on_stats(&stats) {
                        eprintln!("mm-host: {error}");
                    }
                }
                Message::ControlStatus { enabled } => {
                    send(
                        &mut writer,
                        Message::ControlStatus { enabled },
                        config.write_timeout,
                    )
                    .await?
                }
                Message::Bye { .. } => return Ok(()),
                Message::Pong { .. } => {}
                _ => return Err("unexpected viewer message".into()),
            }
        }
        if controls.is_closed() {
            return Err("viewer disconnected".into());
        }
        // Keep next() pinned across renewed deadlines: cancelling it on each
        // Ping could lose the encoder while a blocking encode is in flight.
        let video = {
            let next = pipeline.next(force);
            tokio::pin!(next);
            loop {
                let deadline =
                    *activity.lock().unwrap_or_else(|e| e.into_inner()) + config.idle_timeout;
                if tokio::time::Instant::now() >= deadline {
                    return Err("viewer idle timeout".into());
                }
                let result = tokio::select! { _=disconnected.notified()=>return Err("viewer disconnected".into()), result=tokio::time::timeout_at(deadline,&mut next)=>result };
                match result {
                    Ok(result) => break result?,
                    Err(_) => continue, // Reader may have renewed the deadline.
                }
            }
        };
        let Some(video) = video else {
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
        // Reserve the sequence before writing: a partial write can reach the peer.
        if let Some((_, registry)) = &binding {
            registry.sent(config.session_id, seq);
        }
        let write_started = std::time::Instant::now();
        if writer.send(kind, &payload, config.write_timeout).await? != SendOutcome::Sent {
            return Err("viewer disconnected".into());
        }
        pipeline.write_completed(write_started.elapsed());
        pipeline.sent();
    }
}
