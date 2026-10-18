//! Protocol handling and bounded output, independent of any GUI toolkit.
use async_trait::async_trait;
use mightymonitor_core::*;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

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
}
impl Default for Output {
    fn default() -> Self {
        Self {
            state: State::Connecting,
            stats: Stats::default(),
            picture: None,
            error: None,
        }
    }
}
pub type SharedOutput = Arc<Mutex<Output>>;
struct Dial {
    address: String,
    first: Option<TcpTransport>,
}
#[async_trait]
impl Connector for Dial {
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
        let transport = match self.first.take() {
            Some(t) => t,
            None => TcpTransport::connect(&self.address).await?,
        };
        Ok(Box::new(transport))
    }
}
async fn send_transport(t: &mut TcpTransport, m: Message) -> Result<(), Error> {
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
}
impl<D: Decoder> ViewerSession<D> {
    pub fn new(decoder: D) -> Self {
        Self {
            output: Arc::new(Mutex::new(Output::default())),
            decoder,
        }
    }
    pub async fn run(mut self, address: String) {
        let result = self.drive(address).await;
        let mut out = self.output.lock().unwrap_or_else(|p| p.into_inner());
        out.state = State::Closed;
        if let Err(e) = result {
            out.error = Some(e.to_string());
        }
    }
    async fn drive(&mut self, address: String) -> Result<(), Error> {
        let (transport, id) = loop {
            match tokio::time::timeout(Duration::from_secs(5), async {
                let mut t = TcpTransport::connect(&address).await?;
                send_transport(
                    &mut t,
                    Message::Hello {
                        proto_version: PROTOCOL_VERSION,
                        device_name: "mm-viewer".into(),
                        role: Role::Viewer,
                        capabilities: vec!["h264".into()],
                    },
                )
                .await?;
                let f = t.recv().await?;
                match Message::from_frame_parts(f.kind, &f.payload)? {
                    Message::HelloAck {
                        session_id,
                        accepted: true,
                    } => Ok((t, session_id)),
                    _ => Err::<_, Error>("host rejected handshake".into()),
                }
            })
            .await
            {
                Ok(Ok(v)) => break v,
                Ok(Err(e)) => {
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
        let mut session = ReconnectingSession::new(
            Dial {
                address,
                first: Some(transport),
            },
            id,
            BackoffPolicy::default(),
            SplitMix64::new(id),
        );
        session.connect_initial().await?;
        let (mut reader, mut writer) = session.split();
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
        let (controls, mut commands) = mpsc::channel(8);
        let output = self.output.clone();
        let writer_task = tokio::spawn(async move {
            let mut poll = tokio::time::interval(Duration::from_millis(25));
            loop {
                tokio::select! {
                    command = commands.recv() => match command {
                        Some(message) => { if let Err(e) = send(&mut writer, message).await { output.lock().unwrap_or_else(|p| p.into_inner()).error = Some(e.to_string()); } }
                        None => break,
                    },
                    _ = poll.tick() => {
                        if writer.state() != SessionState::Connected {
                            output.lock().unwrap_or_else(|p| p.into_inner()).state = State::Reconnecting;
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
        let clock = MonotonicClock::new();
        let mut nonce = 0u64;
        let mut ping = None;
        loop {
            tokio::select! {
                result = rx.recv() => {
                    let f = result.ok_or("reader stopped")??;
                    match Message::from_frame_parts(f.kind,&f.payload)? {
                        Message::HelloAck { accepted: true, session_id } if session_id == id => { acknowledged = true; dimensions = None; keyframe_needed = true; }
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
                        Message::Ping { nonce, t_send_us } => controls.try_send(Message::Pong { nonce,t_send_us })?,
                        Message::Pong { nonce: n, t_send_us } => {
                            if let Some((expected,stamp,start)) = ping { if n == expected && t_send_us == stamp { self.output.lock().unwrap_or_else(|p| p.into_inner()).stats.rtt_ms = tokio::time::Instant::now().duration_since(start).as_millis().min(u32::MAX as u128) as u32; ping = None; } }
                        }
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
                            controls.try_send(Message::NetStats(NetStats { rtt_ms: stats.rtt_ms, loss_pct: 0.0, recv_kbps: stats.kbps }))?;
                            nonce = nonce.wrapping_add(1);
                            let t_send_us = clock.now_us();
                            ping = Some((nonce,t_send_us,tokio::time::Instant::now()));
                            controls.try_send(Message::Ping { nonce,t_send_us })?;
                        }
                    }
                }
            }
        }
    }
}
