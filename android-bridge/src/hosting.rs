//! Encoded Android screen hosting. Platform capture stays outside the protocol.
use crate::{
    socket::{Connection, FrameWriter},
    Error, FrameDecoder, Message, MonotonicClock, Role, PROTOCOL_VERSION,
};
use std::{
    collections::VecDeque,
    io::{self, Read},
    net::{SocketAddr, TcpListener},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use trinix_core::{encode_stream_frame, Codec, InputEvent};
use trinix_security::{blocking, Invitation, ShareIdentity};
struct Frame {
    data: Vec<u8>,
    timestamp_us: u64,
    keyframe: bool,
}
struct Shared {
    queue: Mutex<VecDeque<Frame>>,
    stopped: Arc<AtomicBool>,
    connected: AtomicBool,
    control_allowed: AtomicBool,
    inputs: Mutex<VecDeque<InputEvent>>,
    need_keyframe: AtomicBool,
    bitrate: AtomicU32,
    status: Mutex<String>,
}
pub struct ScreenHost {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    address: SocketAddr,
    identity: Arc<ShareIdentity>,
}
impl ScreenHost {
    pub fn start(address: &str, width: u32, height: u32, fps: u32) -> io::Result<Self> {
        if width < 2
            || height < 2
            || width > 8192
            || height > 8192
            || width % 2 != 0
            || height % 2 != 0
            || !(1..=60).contains(&fps)
        {
            return Err(io::Error::other("Unsupported screen size or frame rate"));
        }
        let address: SocketAddr = address
            .parse()
            .map_err(|_| io::Error::other("Invalid sharing address"))?;
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            stopped: Arc::new(AtomicBool::new(false)),
            connected: AtomicBool::new(false),
            control_allowed: AtomicBool::new(false),
            inputs: Mutex::new(VecDeque::new()),
            need_keyframe: AtomicBool::new(true),
            bitrate: AtomicU32::new(4_000_000),
            status: Mutex::new("Ready to connect".into()),
        });
        let identity = Arc::new(ShareIdentity::generate()?);
        let server_identity = identity.clone();
        let state = shared.clone();
        // Session identity changes on each explicitly started sharing session.
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos() as u64;
        let worker = thread::Builder::new()
            .name("mm-android-host".into())
            .spawn(move || {
                while !state.stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            state
                                .queue
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .clear();
                            state.need_keyframe.store(true, Ordering::Relaxed);
                            let result = blocking::accept_cancellable(
                                stream,
                                &server_identity,
                                state.stopped.clone(),
                            )
                            .map(Connection::Secure)
                            .map_err(|e| -> Error { e.into() })
                            .and_then(|stream| serve(&state, stream, id, width, height, fps));
                            state.connected.store(false, Ordering::Relaxed);
                            release_input(&state);
                            state
                                .queue
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .clear();
                            *state.status.lock().unwrap_or_else(|e| e.into_inner()) = match result {
                                Ok(()) => "Ready to connect".into(),
                                Err(e) => format!("Ready to reconnect ({e})"),
                            };
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(e) => {
                            *state.status.lock().unwrap_or_else(|e| e.into_inner()) = e.to_string();
                            break;
                        }
                    }
                }
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
            address,
            identity,
        })
    }
    pub fn invitation(&self, address: SocketAddr) -> io::Result<Invitation> {
        self.identity.invitation(address)
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    /// Nonblocking producer. Overflow discards the reference chain and requires
    /// a fresh independently decodable keyframe, never arbitrary P-frame drops.
    pub fn publish(&self, data: Vec<u8>, timestamp_us: u64, keyframe: bool) -> bool {
        if data.is_empty()
            || data.len() > 8 * 1024 * 1024
            || !self.shared.connected.load(Ordering::Relaxed)
        {
            return false;
        }
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() >= 3 {
            queue.clear();
            self.shared.need_keyframe.store(true, Ordering::Relaxed);
        }
        if self.shared.need_keyframe.load(Ordering::Relaxed) && !keyframe {
            return false;
        }
        if keyframe {
            self.shared.need_keyframe.store(false, Ordering::Relaxed);
        }
        queue.push_back(Frame {
            data,
            timestamp_us,
            keyframe,
        });
        true
    }
    pub fn set_control_allowed(&self, allowed: bool) {
        self.shared
            .control_allowed
            .store(allowed, Ordering::Release);
        if !allowed {
            release_input(&self.shared);
        }
    }
    pub fn poll_input(&self) -> Option<InputEvent> {
        self.shared
            .inputs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
    }
    pub fn needs_keyframe(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
            && self.shared.need_keyframe.load(Ordering::Relaxed)
    }
    pub fn bitrate(&self) -> u32 {
        self.shared.bitrate.load(Ordering::Relaxed)
    }
    pub fn status(&self) -> String {
        self.shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
impl Drop for ScreenHost {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn release_input(shared: &Shared) {
    let mut inputs = shared.inputs.lock().unwrap_or_else(|e| e.into_inner());
    inputs.clear();
    inputs.push_back(InputEvent::ReleaseAll);
}
fn write_message(stream: &mut impl FrameWriter, message: Message) -> Result<(), Error> {
    let (kind, payload) = message.to_frame_parts()?;
    stream.write_frame(
        &encode_stream_frame(kind, &payload)?,
        Duration::from_millis(500),
    )?;
    Ok(())
}
fn serve(
    shared: &Shared,
    mut stream: Connection,
    id: u64,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(), Error> {
    stream.set_read_timeout(Some(Duration::from_millis(15)))?;
    let mut decoder = FrameDecoder::new();
    let mut buffer = [0; 16 * 1024];
    let start = Instant::now();
    let clock = MonotonicClock::new();
    let mut last_control = start;
    let mut acknowledged = false;
    let mut controlling = false;
    let mut seq = 0_u64;
    let mut last_adjustment = start;
    while !shared.stopped.load(Ordering::Relaxed) {
        if (!acknowledged && start.elapsed() > Duration::from_secs(5))
            || last_control.elapsed() > Duration::from_secs(15)
        {
            return Err("Connection timed out".into());
        }
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                decoder.push(&buffer[..n]);
                while let Some(frame) = decoder.next_frame()? {
                    if shared.stopped.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    let message = Message::from_frame_parts(frame.kind, &frame.payload)?;
                    last_control = Instant::now();
                    if !acknowledged {
                        let supports_input = matches!(&message,Message::Hello{capabilities,..} if capabilities.iter().any(|c|c=="input-v1"));
                        let accepted = match message {
                            Message::Hello {
                                proto_version: PROTOCOL_VERSION,
                                role: Role::Viewer,
                                ..
                            } => true,
                            Message::Resume {
                                session_id,
                                last_seq,
                            } if session_id == id => {
                                seq = last_seq;
                                true
                            }
                            _ => false,
                        };
                        write_message(
                            &mut stream,
                            if supports_input {
                                Message::HelloAckV2 {
                                    session_id: id,
                                    accepted,
                                    control_available: shared
                                        .control_allowed
                                        .load(Ordering::Acquire),
                                }
                            } else {
                                Message::HelloAck {
                                    session_id: id,
                                    accepted,
                                }
                            },
                        )?;
                        if !accepted {
                            return Err("Viewer handshake rejected".into());
                        }
                        write_message(
                            &mut stream,
                            Message::VideoConfig {
                                codec: Codec::H264,
                                width,
                                height,
                                fps,
                            },
                        )?;
                        shared.need_keyframe.store(true, Ordering::Relaxed);
                        shared.connected.store(true, Ordering::Relaxed);
                        *shared.status.lock().unwrap_or_else(|e| e.into_inner()) =
                            "Screen connected".into();
                        acknowledged = true;
                    } else {
                        match message {
                            Message::Ping { nonce, t_send_us } => {
                                let t_peer_recv_us = clock.now_us();
                                write_message(
                                    &mut stream,
                                    Message::Pong {
                                        nonce,
                                        t_send_us,
                                        t_peer_recv_us,
                                        t_peer_send_us: clock.now_us(),
                                    },
                                )?
                            }
                            Message::ControlRequest { enabled } => {
                                release_input(shared);
                                controlling =
                                    enabled && shared.control_allowed.load(Ordering::Acquire);
                                write_message(
                                    &mut stream,
                                    Message::ControlStatus {
                                        enabled: controlling,
                                    },
                                )?;
                            }
                            Message::Input(InputEvent::ReleaseAll) => release_input(shared),
                            Message::Input(event)
                                if controlling
                                    && shared.control_allowed.load(Ordering::Acquire)
                                    && event.validate() =>
                            {
                                let mut inputs =
                                    shared.inputs.lock().unwrap_or_else(|e| e.into_inner());
                                if matches!(event, InputEvent::ReleaseAll) {
                                    inputs.clear();
                                }
                                if inputs.len() >= 64 {
                                    drop(inputs);
                                    release_input(shared);
                                    return Err("Remote input queue is full".into());
                                }
                                inputs.push_back(event);
                            }
                            Message::Input(_) => {
                                return Err("Remote input not authorized or invalid".into())
                            }
                            Message::Bye { .. } => return Ok(()),
                            Message::NetStats(stats) => {
                                if last_adjustment.elapsed() >= Duration::from_secs(1)
                                    && stats.loss_pct.is_finite()
                                    && (0.0..=100.0).contains(&stats.loss_pct)
                                {
                                    let current = shared.bitrate.load(Ordering::Relaxed);
                                    let next = if stats.rtt_ms > 180 || stats.loss_pct > 2.0 {
                                        current.saturating_mul(3) / 4
                                    } else {
                                        current.saturating_add(100_000)
                                    };
                                    shared
                                        .bitrate
                                        .store(next.clamp(500_000, 6_000_000), Ordering::Relaxed);
                                    last_adjustment = Instant::now();
                                }
                            }
                            Message::Pong { .. } => {}
                            _ => return Err("Unsupported viewer command".into()),
                        }
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e.into()),
        }
        if controlling && !shared.control_allowed.load(Ordering::Acquire) {
            controlling = false;
            release_input(shared);
            write_message(&mut stream, Message::ControlStatus { enabled: false })?;
        }
        if acknowledged {
            let frame = shared
                .queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pop_front();
            if let Some(frame) = frame {
                seq = seq.checked_add(1).ok_or("Frame sequence exhausted")?;
                write_message(
                    &mut stream,
                    Message::VideoFrame {
                        seq,
                        timestamp_us: frame.timestamp_us,
                        keyframe: frame.keyframe,
                        data: frame.data,
                    },
                )?;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_capture_dimensions() {
        assert!(ScreenHost::start("127.0.0.1:0", 13, 20, 30).is_err());
    }
    #[test]
    fn overflow_requires_a_new_keyframe() {
        let host = ScreenHost::start("127.0.0.1:0", 640, 480, 30).unwrap();
        host.shared.connected.store(true, Ordering::Relaxed);
        assert!(!host.publish(vec![1], 0, false));
        assert!(host.publish(vec![1], 0, true));
        assert!(host.publish(vec![2], 1, false));
        assert!(host.publish(vec![3], 2, false));
        assert!(!host.publish(vec![4], 3, false));
        assert!(host.needs_keyframe());
        assert!(host.publish(vec![5], 4, true));
        assert_eq!(host.shared.queue.lock().unwrap().len(), 1);
    }
    #[test]
    fn secure_android_client_receives_video_from_secure_android_host() {
        let host = ScreenHost::start("127.0.0.1:0", 640, 480, 30).unwrap();
        let client =
            crate::Client::connect(&host.invitation(host.address()).unwrap().to_link()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut configured = false;
        while Instant::now() < deadline {
            host.publish(vec![0, 0, 0, 1, 0x65], 42, true);
            if let Some(event) = client.poll() {
                if event[0] == 2 {
                    configured = true;
                }
                if event[0] == 3 {
                    assert!(configured);
                    assert_eq!(&event[1..9], &42_u64.to_le_bytes());
                    return;
                }
                assert_ne!(event[0], 5, "Secure host authentication failed");
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("Secure Android client never received video");
    }
    #[test]
    fn stop_cancels_an_unauthenticated_stalled_peer() {
        let host = ScreenHost::start("127.0.0.1:0", 640, 480, 30).unwrap();
        let _silent = std::net::TcpStream::connect(host.address()).unwrap();
        thread::sleep(Duration::from_millis(50));
        let start = Instant::now();
        drop(host);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn phone_control_requires_consent_and_releases_on_revocation() {
        let host = ScreenHost::start("127.0.0.1:0", 640, 480, 30).unwrap();
        host.set_control_allowed(true);
        let client =
            crate::Client::connect(&host.invitation(host.address()).unwrap().to_link()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(Instant::now() < deadline);
            if let Some(event) = client.poll() {
                if event == [6, 1, 0] {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        client.request_control(true).unwrap();
        loop {
            assert!(Instant::now() < deadline);
            if let Some(event) = client.poll() {
                if event == [6, 1, 1] {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        client.input(InputEvent::Text("test".into())).unwrap();
        loop {
            assert!(Instant::now() < deadline);
            if let Some(InputEvent::Text(text)) = host.poll_input() {
                assert_eq!(text, "test");
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        host.set_control_allowed(false);
        assert_eq!(host.poll_input(), Some(InputEvent::ReleaseAll));
        loop {
            assert!(Instant::now() < deadline);
            if let Some(event) = client.poll() {
                if event == [6, 1, 0] {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(client
            .input(InputEvent::Key {
                code: 4,
                pressed: true
            })
            .is_err());
    }
    #[test]
    fn viewer_receives_config_keyframe_and_pong() {
        let host = ScreenHost::start("127.0.0.1:0", 640, 480, 30).unwrap();
        let invitation = host.invitation(host.address()).unwrap();
        let mut stream = Connection::Secure(
            blocking::connect(
                std::net::TcpStream::connect(host.address()).unwrap(),
                &invitation,
            )
            .unwrap(),
        );
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write_message(
            &mut stream,
            Message::Hello {
                proto_version: PROTOCOL_VERSION,
                device_name: "test".into(),
                role: Role::Viewer,
                capabilities: vec!["h264".into()],
            },
        )
        .unwrap();
        let mut decoder = FrameDecoder::new();
        let mut bytes = [0; 8192];
        let mut configured = false;
        let mut video = false;
        let mut pong = false;
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end && !(video && pong) {
            if configured {
                host.publish(vec![0, 0, 0, 1, 0x65, 1], 42, true);
                write_message(
                    &mut stream,
                    Message::Ping {
                        nonce: 7,
                        t_send_us: 9,
                    },
                )
                .unwrap();
            }
            let n = stream.read(&mut bytes).unwrap();
            decoder.push(&bytes[..n]);
            while let Some(frame) = decoder.next_frame().unwrap() {
                match Message::from_frame_parts(frame.kind, &frame.payload).unwrap() {
                    Message::VideoConfig {
                        width: 640,
                        height: 480,
                        ..
                    } => configured = true,
                    Message::VideoFrame {
                        keyframe: true,
                        timestamp_us: 42,
                        ..
                    } => video = true,
                    Message::Pong { nonce: 7, .. } => pong = true,
                    _ => {}
                }
            }
        }
        assert!(configured && video && pong);
        write_message(
            &mut stream,
            Message::Bye {
                reason: "done".into(),
            },
        )
        .unwrap();
    }
}
