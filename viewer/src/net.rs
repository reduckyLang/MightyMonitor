//! Protocol handling and bounded output, independent of any GUI toolkit.
use async_trait::async_trait;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use trinix_core::*;

type Error = Box<dyn std::error::Error + Send + Sync>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Connecting,
    Connected,
    Reconnecting,
    Closed,
}
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub fps: f64,
    pub kbps: u32,
    pub rtt_ms: u32,
    pub present_ms: f64,
}
pub struct Picture {
    pub i420: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub decoded: Instant,
}
pub struct Output {
    pub state: State,
    pub stats: Stats,
    pub picture: Option<Picture>,
    pub error: Option<String>,
    pub control_available: bool,
    pub controlling: bool,
    pub control_epoch: u64,
    pub control_response: Option<bool>,
}
impl Default for Output {
    fn default() -> Self {
        Self {
            state: State::Connecting,
            stats: Stats::default(),
            picture: None,
            error: None,
            control_available: false,
            controlling: false,
            control_epoch: 0,
            control_response: None,
        }
    }
}
#[derive(Debug)]
pub struct Command {
    pub generation: u64,
    pub action: ControlAction,
}
#[derive(Debug)]
pub enum ControlAction {
    Control(bool),
    Input(InputEvent),
}
pub type SharedOutput = Arc<Mutex<Output>>;
enum SessionEndpoint {
    Link(trinix_security::Endpoint),
    Paired {
        address: std::net::SocketAddr,
        directory: std::path::PathBuf,
        fingerprint: [u8; 32],
    },
}
impl SessionEndpoint {
    fn is_secure(&self) -> bool {
        match self {
            Self::Link(endpoint) => endpoint.is_secure(),
            Self::Paired { .. } => true,
        }
    }
    async fn connect(&self) -> Result<Box<dyn Transport>, TransportError> {
        match self {
            Self::Link(endpoint) => {
                trinix_security::asynchronous::connect_transport(endpoint).await
            }
            Self::Paired {
                address,
                directory,
                fingerprint,
            } => {
                let identity = trinix_security::pairing::DeviceIdentity::open(directory)
                    .map_err(TransportError::Io)?;
                let store = trinix_security::store::DeviceStore::open(directory)
                    .map_err(TransportError::Io)?;
                let peer = store
                    .peers()
                    .iter()
                    .find(|peer| &peer.fingerprint == fingerprint)
                    .ok_or_else(|| {
                        TransportError::Authentication("Pair this device again.".into())
                    })?;
                let stream = crate::devices::dial(*address, false)
                    .await
                    .map_err(TransportError::Io)?;
                let tls =
                    trinix_security::pairing::asynchronous::connect(stream, &identity, peer)
                        .await
                        .map_err(|_| {
                            TransportError::Authentication(
                                "This device has changed. Pair it again.".into(),
                            )
                        })?;
                Ok(Box::new(FramedTransport::from_stream(tls)))
            }
        }
    }
}
struct Dial {
    endpoint: SessionEndpoint,
    first: Option<Box<dyn Transport>>,
}
#[async_trait]
impl Connector for Dial {
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
        let transport = match self.first.take() {
            Some(t) => t,
            None => self.endpoint.connect().await?,
        };
        Ok(transport)
    }
}
async fn send_transport(t: &mut dyn Transport, m: Message) -> Result<(), Error> {
    let (k, p) = m.to_frame_parts()?;
    t.send(k, &p).await?;
    Ok(())
}
async fn send(w: &mut SessionWriter, m: Message) -> Result<(), Error> {
    let (k, p) = m.to_frame_parts()?;
    // Recovery owns Resume; periodic controls are disposable during an outage.
    w.send(k, &p, Duration::from_secs(2)).await?;
    Ok(())
}
/// Ordered video session with a latest-picture output slot.
/// The decoder must return packed, even-dimension I420 matching VideoConfig.
/// Run on a worker runtime: the synchronous Decoder may block during decoding.
pub struct ViewerSession<D: Decoder> {
    pub output: SharedOutput,
    decoder: D,
    pub commands: mpsc::Sender<Command>,
    control_rx: mpsc::Receiver<Command>,
}
impl<D: Decoder> ViewerSession<D> {
    pub fn new(decoder: D) -> Self {
        let (commands, control_rx) = mpsc::channel(64);
        Self {
            output: Arc::new(Mutex::new(Output::default())),
            decoder,
            commands,
            control_rx,
        }
    }
    pub async fn run(mut self, address: String) {
        let result = match trinix_security::Endpoint::parse(&address, true) {
            Ok(endpoint) => self.drive(SessionEndpoint::Link(endpoint)).await,
            Err(error) => Err(error.into()),
        };
        let mut out = self.output.lock().unwrap_or_else(|p| p.into_inner());
        out.state = State::Closed;
        out.controlling = false;
        if let Err(e) = result {
            out.error = Some(e.to_string());
        }
    }
    pub async fn run_paired(
        mut self,
        address: std::net::SocketAddr,
        directory: std::path::PathBuf,
        fingerprint: [u8; 32],
    ) {
        let result = self
            .drive(SessionEndpoint::Paired {
                address,
                directory,
                fingerprint,
            })
            .await;
        let mut out = self.output.lock().unwrap_or_else(|p| p.into_inner());
        out.state = State::Closed;
        out.controlling = false;
        if result.is_err() {
            out.error = Some("Couldn't connect. Pair this device again.".into());
        }
    }
    async fn drive(&mut self, endpoint: SessionEndpoint) -> Result<(), Error> {
        let handshake_timeout = Duration::from_secs(if endpoint.is_secure() { 15 } else { 5 });
        let (transport, id, control_available) = loop {
            match tokio::time::timeout(handshake_timeout, async {
                let mut t = endpoint.connect().await?;
                send_transport(
                    t.as_mut(),
                    Message::Hello {
                        proto_version: PROTOCOL_VERSION,
                        device_name: "trinix-viewer".into(),
                        role: Role::Viewer,
                        capabilities: vec!["h264".into(), "input-v1".into()],
                    },
                )
                .await?;
                let f = t.recv().await?;
                match Message::from_frame_parts(f.kind, &f.payload)? {
                    Message::HelloAck {
                        session_id,
                        accepted: true,
                    } => Ok((t, session_id, false)),
                    Message::HelloAckV2 {
                        session_id,
                        accepted: true,
                        control_available,
                    } => Ok((t, session_id, control_available)),
                    _ => Err::<_, Error>("host rejected handshake".into()),
                }
            })
            .await
            {
                Ok(Ok(v)) => break v,
                Ok(Err(e)) => {
                    if matches!(
                        e.downcast_ref::<TransportError>(),
                        Some(TransportError::Authentication(_))
                    ) {
                        return Err(e);
                    }
                    self.output.lock().unwrap_or_else(|p| p.into_inner()).error =
                        Some(e.to_string());
                }
                Err(e) => {
                    self.output.lock().unwrap_or_else(|p| p.into_inner()).error =
                        Some(e.to_string());
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        let control_available = control_available && endpoint.is_secure();
        self.output
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .control_available = control_available;
        let mut session = ReconnectingSession::new(
            Dial {
                endpoint,
                first: Some(transport),
            },
            id,
            BackoffPolicy::default(),
            SplitMix64::new(id),
        );
        session.connect_initial().await?;
        let (mut reader, mut writer) = session.split();
        self.output
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .control_epoch = writer.generation();
        let clock = MonotonicClock::new();
        let writer_clock = clock.clone();
        let (tx, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(async move {
            loop {
                let result = reader.recv().await;
                if let Ok(ref f) = result {
                    if let Ok(Message::VideoFrame { seq, .. }) =
                        Message::from_frame_parts(f.kind, &f.payload)
                    {
                        reader.set_last_seq(seq);
                    }
                }
                let failed = result.is_err();
                if tx.send(result).await.is_err() || failed {
                    break;
                }
            }
        });
        struct Abort(tokio::task::JoinHandle<()>);
        impl Drop for Abort {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _task = Abort(task);
        let (controls, mut commands) = mpsc::channel(128);
        let output = self.output.clone();
        let writer_task = tokio::spawn(async move {
            let mut poll = tokio::time::interval(Duration::from_millis(25));
            loop {
                tokio::select! {
                    command = commands.recv() => match command {
                        Some((generation,mut message)) if generation==writer.generation() => {
                            if let Message::Pong { t_peer_send_us, .. } = &mut message {
                                *t_peer_send_us = writer_clock.now_us();
                            }
                            if let Err(e) = send(&mut writer, message).await { output.lock().unwrap_or_else(|p| p.into_inner()).error = Some(e.to_string()); }
                        }
                        Some(_)=>{},
                        None => break,
                    },
                    _ = poll.tick() => {
                        {let mut out=output.lock().unwrap_or_else(|e|e.into_inner());if out.control_epoch!=writer.generation() {out.control_epoch=writer.generation();out.controlling=false;}}
                        if writer.state() != SessionState::Connected {
                            let mut out=output.lock().unwrap_or_else(|p| p.into_inner()); out.state = State::Reconnecting; out.controlling=false;
                        }
                    }
                }
            }
        });
        let _writer_task = Abort(writer_task);

        let mut tick = tokio::time::interval(Duration::from_millis(50));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut window = tokio::time::Instant::now();
        let mut bytes = 0u64;
        let mut frames = 0u64;
        let mut dimensions = None;
        let mut acknowledged = true;
        let mut keyframe_needed = true;
        let mut nonce = 0u64;
        let mut ping = None;
        loop {
            tokio::select! {
                command=self.control_rx.recv() => {
                    let Some(command)=command else {return Err("Viewer controls closed".into())};
                    let (connected,controlling,epoch)={let out=self.output.lock().unwrap_or_else(|e|e.into_inner());(out.state==State::Connected,out.controlling,out.control_epoch)};
                    if command.generation!=epoch {continue;}
                    match command.action {
                        ControlAction::Control(enabled) if connected && control_available => {{let mut out=self.output.lock().unwrap_or_else(|e|e.into_inner());out.control_response=None;if !enabled {out.controlling=false;}} controls.try_send((epoch,Message::ControlRequest {enabled}))?;},
                        ControlAction::Input(event) if connected && (controlling || matches!(event,InputEvent::ReleaseAll)) && event.validate()=>{controls.try_send((epoch,Message::Input(event)))?;},
                        _=>{},
                    }
                }
                result = rx.recv() => {
                    let f = result.ok_or("reader stopped")??;
                    match Message::from_frame_parts(f.kind,&f.payload)? {
                        Message::HelloAck { accepted: true, session_id } if session_id == id => { acknowledged = true; dimensions = None; keyframe_needed = true; self.output.lock().unwrap_or_else(|e|e.into_inner()).controlling=false; }
                        Message::VideoConfig { codec: Codec::H264, width, height, .. } if acknowledged && width > 0 && height > 0 && width <= 8192 && height <= 8192 && width % 2 == 0 && height % 2 == 0 => { dimensions = Some((width,height)); let mut o = self.output.lock().unwrap_or_else(|p| p.into_inner()); o.state = State::Connected; o.error = None; }
                        Message::VideoFrame { data, keyframe, .. } => {
                            let (width,height) = dimensions.ok_or("video before configuration")?;
                            bytes += data.len() as u64;
                            if keyframe_needed && !keyframe { continue; }
                            let i420 = self.decoder.decode(&data)?;
                            if i420.len() != width as usize * height as usize * 3 / 2 { return Err("decoded frame size differs from VideoConfig".into()); }
                            keyframe_needed = false; frames += 1;
                            self.output.lock().unwrap_or_else(|p| p.into_inner()).picture = Some(Picture { i420, width, height, decoded: Instant::now() });
                        }
                        Message::Ping { nonce, t_send_us } => {
                            let t_peer_recv_us = clock.now_us();
                            controls.try_send((self.output.lock().unwrap_or_else(|e|e.into_inner()).control_epoch,Message::Pong { nonce,t_send_us,t_peer_recv_us,t_peer_send_us:0 }))?
                        }
                        Message::Pong { nonce: n, t_send_us, t_peer_recv_us, t_peer_send_us } => {
                            if let Some((expected,stamp)) = ping { if n == expected && t_send_us == stamp {
                                let rtt_ms = pong_rtt_us(stamp, t_peer_recv_us, t_peer_send_us, clock.now_us()) / 1_000;
                                self.output.lock().unwrap_or_else(|p| p.into_inner()).stats.rtt_ms = rtt_ms.min(u64::from(u32::MAX)) as u32;
                                ping = None;
                            } }
                        }
                        Message::ControlStatus {enabled} if control_available => {let mut out=self.output.lock().unwrap_or_else(|e|e.into_inner());out.controlling=enabled;out.control_response=Some(enabled);},
                        Message::Bye { reason } => return Err(reason.into()),
                        _ => return Err("unexpected host message or unsupported video configuration".into()),
                    }
                }
                _ = tick.tick() => {
                    if window.elapsed() >= Duration::from_secs(1) {
                        let elapsed = window.elapsed().as_secs_f64();
                        window = tokio::time::Instant::now();
                        let stats = {
                            let mut o = self.output.lock().unwrap_or_else(|p| p.into_inner());
                            o.stats.fps = frames as f64 / elapsed;
                            o.stats.kbps = (bytes as f64 * 8.0 / elapsed / 1000.0).min(u32::MAX as f64) as u32;
                            o.stats.clone()
                        };
                        bytes = 0; frames = 0;
                        if dimensions.is_some() {
                            controls.try_send((self.output.lock().unwrap_or_else(|e|e.into_inner()).control_epoch,Message::NetStats(NetStats { rtt_ms: stats.rtt_ms, loss_pct: 0.0, recv_kbps: stats.kbps })))?;
                            nonce = nonce.wrapping_add(1);
                            let t_send_us = clock.now_us();
                            ping = Some((nonce,t_send_us));
                            controls.try_send((self.output.lock().unwrap_or_else(|e|e.into_inner()).control_epoch,Message::Ping { nonce,t_send_us }))?;
                        }
                    }
                }
            }
        }
    }
}
