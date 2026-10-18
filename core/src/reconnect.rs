//! Frame-level reconnect, explicit resume, and deterministic backoff.

use crate::{
    Frame, FrameError, Message, MessageError, TcpTransport, Transport, TransportError,
    MAX_PAYLOAD_LEN,
};
use async_trait::async_trait;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

/// Establish a fresh transport on each attempt.
#[async_trait]
pub trait Connector: Send {
    /// Resolve the destination anew and connect.
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError>;
}

/// TCP connector retaining a hostname and port rather than a resolved IP.
pub struct TcpConnector {
    host: String,
    timeout: Duration,
}
impl TcpConnector {
    /// Store a `host:port` address; each attempt resolves it through Tokio ToSocketAddrs.
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            timeout: Duration::from_secs(5),
        }
    }
    /// Set the deadline covering resolution and connection.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}
#[async_trait]
impl Connector for TcpConnector {
    async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
        Ok(Box::new(
            TcpTransport::connect_with_timeout(self.host.as_str(), self.timeout).await?,
        ))
    }
}

/// Injectable source of uniform values in `[0, 1)`.
pub trait JitterSource: Send {
    /// Return the next unit sample.
    fn next_unit(&mut self) -> f64;
}
/// Dependency-free, reproducible SplitMix64 generator (not cryptographic).
pub struct SplitMix64(u64);
impl SplitMix64 {
    /// Initialize a reproducible sequence.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
}
impl JitterSource for SplitMix64 {
    fn next_unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Exponential retry configuration; use builder methods to normalize float inputs.
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    initial: Duration,
    max: Duration,
    multiplier: f64,
    jitter: f64,
    max_attempts: Option<u32>,
}
impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(5),
            multiplier: 2.0,
            jitter: 0.2,
            max_attempts: None,
        }
    }
}
impl BackoffPolicy {
    /// Set the first retry delay (also subject to the cap).
    pub fn with_initial(mut self, initial: Duration) -> Self {
        self.initial = initial;
        self
    }
    /// Set the hard delay cap, including jitter; values below 1 ms become 1 ms.
    pub fn with_max(mut self, max: Duration) -> Self {
        self.max = max.max(Duration::from_millis(1));
        self
    }
    /// Set exponential growth; nonfinite values or values below one become one.
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = if multiplier.is_finite() {
            multiplier.max(1.0)
        } else {
            1.0
        };
        self
    }
    /// Set symmetric fractional jitter, clamped to 0..=1; nonfinite becomes zero.
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = if jitter.is_finite() {
            jitter.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }
    /// Limit reconnect attempts per outage; `None` retries indefinitely.
    pub fn with_max_attempts(mut self, max_attempts: Option<u32>) -> Self {
        self.max_attempts = max_attempts;
        self
    }
}

/// Stateful delay iterator. Reset restarts delays and attempt count, not the RNG.
pub struct Backoff<J: JitterSource> {
    policy: BackoffPolicy,
    jitter: J,
    attempts: u32,
    base: Duration,
}
impl<J: JitterSource> Backoff<J> {
    /// Create an iterator with an injected source of randomness.
    pub fn new(policy: BackoffPolicy, jitter: J) -> Self {
        let base = policy.initial.min(policy.max);
        Self {
            policy,
            jitter,
            attempts: 0,
            base,
        }
    }
    /// Restart the retry budget and exponential schedule.
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.base = self.policy.initial.min(self.policy.max);
    }
}
impl<J: JitterSource> Iterator for Backoff<J> {
    type Item = Duration;
    fn next(&mut self) -> Option<Duration> {
        if self
            .policy
            .max_attempts
            .is_some_and(|max| self.attempts >= max)
        {
            return None;
        }
        self.attempts = self.attempts.saturating_add(1);
        let sample = self.jitter.next_unit();
        let sample = if sample.is_finite() {
            sample.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let cap = self.policy.max.as_secs_f64();
        let seconds =
            (self.base.as_secs_f64() * (1.0 + self.policy.jitter * (2.0 * sample - 1.0))).min(cap);
        // Fallible conversion also handles rounding at Duration::MAX.
        let delay = Duration::try_from_secs_f64(seconds)
            .unwrap_or(self.policy.max)
            .min(self.policy.max)
            .max(Duration::from_millis(1));
        self.base = Duration::try_from_secs_f64(
            (self.base.as_secs_f64() * self.policy.multiplier).min(cap),
        )
        .unwrap_or(self.policy.max)
        .min(self.policy.max);
        Some(delay)
    }
}

/// Observable connection lifecycle. Before initial connection, attempt is zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// A usable transport exists.
    Connected,
    /// Awaiting or performing recovery.
    Reconnecting {
        /// One-based reconnect attempt, or zero before the first attempt.
        attempt: u32,
    },
    /// The retry budget was exhausted.
    GaveUp,
}
/// Lifecycle notifications; the session retains only the newest 64 events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// A transport became usable.
    Connected,
    /// The old transport was dropped.
    Disconnected {
        /// Display text for the triggering failure.
        reason: String,
    },
    /// An attempt was scheduled before its sleep.
    ReconnectAttempt {
        /// One-based attempt.
        attempt: u32,
        /// Scheduled sleep.
        delay: Duration,
    },
    /// Connection or sending Resume failed.
    ReconnectFailed {
        /// One-based attempt.
        attempt: u32,
        /// Display text for the failure.
        error: String,
    },
    /// Resume was sent (no acknowledgement is implied).
    Resumed {
        /// Requested replay position.
        last_seq: u64,
    },
    /// Retry budget exhausted.
    GaveUp {
        /// Attempts scheduled during this outage, including cancelled sleeps.
        attempts: u32,
    },
}
/// Session operation failure.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Typed resume payload encoding failed.
    #[error(transparent)]
    Message(#[from] MessageError),
    /// Transport error that did not trigger recovery, including read timeouts.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// Call connect_initial before sending or receiving.
    #[error("session has not connected yet")]
    NotConnected,
    /// Retry budget exhausted; create a new session to restart.
    #[error("reconnect attempts exhausted")]
    GaveUp,
}
/// Result of a send; application frames are never automatically replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// The frame was written successfully.
    Sent,
    /// Recovery succeeded, but the frame was not sent on the new connection.
    /// The original connection may have received some or all of it.
    ReconnectedNotSent,
    /// No transport was available; nothing was queued or written.
    NotConnected,
}

/// Reconnects frame transports and sends Resume first on every replacement.
///
/// Callers maintain the last consumed sequence. Receives transparently recover;
/// sends report recovery without replay. Canceling a send drops its transport;
/// the next operation recovers. Recovery cancellation retains its retry budget,
/// including attempts scheduled before a cancelled sleep. Backoff resets only
/// after receiving a frame, so a successful Resume write alone does not reset it.
pub struct ReconnectingSession<C: Connector, J: JitterSource> {
    connector: C,
    backoff: Backoff<J>,
    session_id: u64,
    last_seq: u64,
    current: Option<Box<dyn Transport>>,
    state: SessionState,
    started: bool,
    events: VecDeque<SessionEvent>,
    shared: Option<Arc<Mutex<SplitStatus>>>,
}
impl<C: Connector, J: JitterSource> ReconnectingSession<C, J> {
    /// Create an unconnected session with explicit policy and RNG.
    pub fn new(connector: C, session_id: u64, policy: BackoffPolicy, jitter: J) -> Self {
        Self {
            connector,
            backoff: Backoff::new(policy, jitter),
            session_id,
            last_seq: 0,
            current: None,
            state: SessionState::Reconnecting { attempt: 0 },
            started: false,
            events: VecDeque::new(),
            shared: None,
        }
    }
    /// Split into independently movable handles. Drive `reader.recv()` to service
    /// writes; dropping either handle closes its channel. Requires no spawned actor.
    pub fn split(mut self) -> (SessionReader<C, J>, SessionWriter) {
        let shared = Arc::new(Mutex::new(SplitStatus {
            state: self.state,
            generation: 0,
            events: std::mem::take(&mut self.events),
        }));
        self.shared = Some(shared.clone());
        let (sender, commands) = mpsc::channel(1);
        (
            SessionReader {
                session: self,
                commands,
                shared: shared.clone(),
            },
            SessionWriter { sender, shared },
        )
    }
    /// Set the last sequence consumed by the caller, to request replay after it.
    /// Defaults to zero; zero requests replay after sequence zero, with no sentinel semantics.
    pub fn set_last_seq(&mut self, last_seq: u64) {
        self.last_seq = last_seq;
    }
    /// Inspect the connection lifecycle.
    pub fn state(&self) -> SessionState {
        self.state
    }
    /// Drain retained events in order; if full, the oldest events were discarded.
    pub fn take_events(&mut self) -> Vec<SessionEvent> {
        self.events.drain(..).collect()
    }
    fn emit(&mut self, event: SessionEvent) {
        if let Some(shared) = &self.shared {
            let mut status = shared.lock().unwrap();
            status.state = self.state;
            if matches!(event, SessionEvent::Disconnected { .. }) {
                status.generation += 1;
            }
            if status.events.len() == 64 {
                status.events.pop_front();
            }
            status.events.push_back(event.clone());
        }
        if self.events.len() == 64 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
    /// Perform one initial connection, without Resume or backoff. Safe to repeat.
    /// Initial failures are returned directly and may be retried by the caller.
    pub async fn connect_initial(&mut self) -> Result<(), SessionError> {
        if self.state == SessionState::GaveUp {
            return Err(SessionError::GaveUp);
        }
        if self.current.is_some() {
            return Ok(());
        }
        if self.started {
            return self.recover().await;
        }
        self.current = Some(self.connector.connect().await?);
        self.started = true;
        self.state = SessionState::Connected;
        self.emit(SessionEvent::Connected);
        Ok(())
    }
    fn disconnect(&mut self, error: &TransportError) {
        self.current = None;
        self.state = SessionState::Reconnecting {
            attempt: self.backoff.attempts,
        };
        self.emit(SessionEvent::Disconnected {
            reason: error.to_string(),
        });
    }
    async fn ready(&mut self) -> Result<(), SessionError> {
        if self.state == SessionState::GaveUp {
            return Err(SessionError::GaveUp);
        }
        if !self.started {
            return Err(SessionError::NotConnected);
        }
        if self.current.is_none() {
            self.recover().await?;
        }
        Ok(())
    }
    async fn recover(&mut self) -> Result<(), SessionError> {
        loop {
            let Some(delay) = self.backoff.next() else {
                self.state = SessionState::GaveUp;
                self.emit(SessionEvent::GaveUp {
                    attempts: self.backoff.attempts,
                });
                return Err(SessionError::GaveUp);
            };
            let attempt = self.backoff.attempts;
            self.state = SessionState::Reconnecting { attempt };
            self.emit(SessionEvent::ReconnectAttempt { attempt, delay });
            tokio::time::sleep(delay).await;
            let result = async {
                let mut transport = self.connector.connect().await?;
                let (kind, payload) = Message::Resume {
                    session_id: self.session_id,
                    last_seq: self.last_seq,
                }
                .to_frame_parts()?;
                transport.send(kind, &payload).await?;
                Ok::<_, SessionError>(transport)
            }
            .await;
            match result {
                Ok(transport) => {
                    self.current = Some(transport);
                    self.state = SessionState::Connected;
                    self.emit(SessionEvent::Connected);
                    self.emit(SessionEvent::Resumed {
                        last_seq: self.last_seq,
                    });
                    return Ok(());
                }
                Err(error) => self.emit(SessionEvent::ReconnectFailed {
                    attempt,
                    error: error.to_string(),
                }),
            }
        }
    }
    /// Send once. After recovery return ReconnectedNotSent for caller-controlled retry.
    pub async fn send(&mut self, kind: u8, payload: &[u8]) -> Result<SendOutcome, SessionError> {
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(TransportError::Frame(FrameError::TooLarge {
                len: payload.len() as u64,
                max: MAX_PAYLOAD_LEN as u64,
            })
            .into());
        }
        let recovering = self.started && self.current.is_none();
        self.ready().await?;
        if recovering {
            return Ok(SendOutcome::ReconnectedNotSent);
        }
        let Some(mut transport) = self.current.take() else {
            return Err(SessionError::NotConnected);
        };
        self.state = SessionState::Reconnecting {
            attempt: self.backoff.attempts,
        };
        match transport.send(kind, payload).await {
            Ok(()) => {
                self.current = Some(transport);
                self.state = SessionState::Connected;
                Ok(SendOutcome::Sent)
            }
            Err(error @ (TransportError::Io(_) | TransportError::Timeout("write"))) => {
                drop(transport);
                self.disconnect(&error);
                self.recover().await?;
                Ok(SendOutcome::ReconnectedNotSent)
            }
            Err(error) => {
                self.current = Some(transport);
                self.state = SessionState::Connected;
                Err(error.into())
            }
        }
    }
    /// Receive, recovering from broken connections. Read timeouts retain the transport.
    pub async fn recv(&mut self) -> Result<Frame, SessionError> {
        loop {
            self.ready().await?;
            let result = match self.current.as_mut() {
                Some(transport) => transport.recv().await,
                None => return Err(SessionError::NotConnected),
            };
            match result {
                Ok(frame) => {
                    self.backoff.reset();
                    return Ok(frame);
                }
                Err(error @ TransportError::Timeout("read")) => return Err(error.into()),
                Err(error) => {
                    self.disconnect(&error);
                    self.recover().await?;
                }
            }
        }
    }
}

struct SplitStatus {
    generation: u64,
    state: SessionState,
    events: VecDeque<SessionEvent>,
}
struct SendCommand {
    started: Arc<AtomicBool>,
    generation: u64,
    kind: u8,
    payload: Vec<u8>,
    reply: oneshot::Sender<Result<SendOutcome, SessionError>>,
}

/// Owns receive and recovery. Poll `recv` to service writer commands.
pub struct SessionReader<C: Connector, J: JitterSource> {
    session: ReconnectingSession<C, J>,
    commands: mpsc::Receiver<SendCommand>,
    shared: Arc<Mutex<SplitStatus>>,
}
/// Sends through the reader's current transport; never initiates recovery.
pub struct SessionWriter {
    sender: mpsc::Sender<SendCommand>,
    shared: Arc<Mutex<SplitStatus>>,
}
// Armed only while the transport is owned by an inline write future.
struct InlineWriteGuard<'a, C: Connector, J: JitterSource>(
    Option<&'a mut ReconnectingSession<C, J>>,
);
impl<'a, C: Connector, J: JitterSource> InlineWriteGuard<'a, C, J> {
    fn finish(mut self) -> &'a mut ReconnectingSession<C, J> {
        self.0.take().unwrap()
    }
}
impl<C: Connector, J: JitterSource> Drop for InlineWriteGuard<'_, C, J> {
    fn drop(&mut self) {
        if let Some(session) = self.0.take() {
            session.disconnect(&TransportError::Closed);
        }
    }
}
impl<C: Connector, J: JitterSource> SessionReader<C, J> {
    /// Establish the initial connection, without Resume.
    pub async fn connect_initial(&mut self) -> Result<(), SessionError> {
        self.session.connect_initial().await
    }
    /// Update the application-consumed position for the next Resume.
    pub fn set_last_seq(&mut self, last_seq: u64) {
        self.session.set_last_seq(last_seq);
    }
    /// Inspect the shared connection lifecycle.
    pub fn state(&self) -> SessionState {
        self.shared.lock().unwrap().state
    }
    /// Drain the shared, bounded event history.
    pub fn take_events(&mut self) -> Vec<SessionEvent> {
        self.shared.lock().unwrap().events.drain(..).collect()
    }
    /// Receive with recovery, servicing writes while a transport is connected.
    ///
    /// Cancellation while reading retains the transport. Cancellation during an
    /// inline write drops the transport and emits Disconnected with Closed as its
    /// reason, since output may be partial. The next receive recovers without
    /// replaying the interrupted command. Recovery retains its retry budget.
    pub async fn recv(&mut self) -> Result<Frame, SessionError> {
        loop {
            self.session.ready().await?;
            tokio::select! {
                command = self.commands.recv(), if !self.commands.is_closed() || !self.commands.is_empty() => {
                    if let Some(mut command) = command {
                        if command.reply.is_closed() { continue; }
                        if command.generation != self.shared.lock().unwrap().generation {
                            let _ = command.reply.send(Ok(SendOutcome::NotConnected));
                            continue;
                        }
                        let mut transport = self.session.current.take().ok_or(SessionError::NotConnected)?;
                        command.started.store(true, Ordering::SeqCst);
                        let guard = InlineWriteGuard(Some(&mut self.session));
                        // Cancellation may leave partial output. Never restore this transport.
                        let result = tokio::select! {
                            result = transport.send(command.kind, &command.payload) => Some(result),
                            _ = command.reply.closed() => None,
                        };
                        let session = guard.finish();
                        match result {
                            Some(Ok(())) => {
                                session.current = Some(transport);
                                let _ = command.reply.send(Ok(SendOutcome::Sent));
                            }
                            Some(Err(error @ (TransportError::Io(_) | TransportError::Timeout("write")))) => {
                                session.disconnect(&error);
                                let _ = command.reply.send(Err(error.into()));
                            }
                            Some(Err(error)) => {
                                session.current = Some(transport);
                                let _ = command.reply.send(Err(error.into()));
                            }
                            None => session.disconnect(&TransportError::Closed),
                        }
                    }
                }
                result = self.session.current.as_mut().unwrap().recv() => {
                    match result {
                        Ok(frame) => { self.session.backoff.reset(); return Ok(frame); }
                        Err(error @ TransportError::Timeout("read")) => return Err(error.into()),
                        Err(error) => self.session.disconnect(&error),
                    }
                }
            }
        }
    }
}
impl SessionWriter {
    /// Inspect the shared connection lifecycle.
    pub fn state(&self) -> SessionState {
        self.shared.lock().unwrap().state
    }
    /// Drain the shared, bounded event history (also consumed by the reader).
    pub fn take_events(&mut self) -> Vec<SessionEvent> {
        self.shared.lock().unwrap().events.drain(..).collect()
    }
    /// Send once, with a caller-supplied deadline covering dispatch and writing.
    /// The reader must be polling `recv`. Timeout/cancellation during a write
    /// drops the transport; no application frame is replayed. Expiry before the
    /// reader starts writing returns Timeout("queue"); expiry after dispatch
    /// returns Timeout("write"). Both phases share the supplied deadline.
    pub async fn send(
        &mut self,
        kind: u8,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<SendOutcome, SessionError> {
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(TransportError::Frame(FrameError::TooLarge {
                len: payload.len() as u64,
                max: MAX_PAYLOAD_LEN as u64,
            })
            .into());
        }
        let (state, generation) = {
            let status = self.shared.lock().unwrap();
            (status.state, status.generation)
        };
        match state {
            SessionState::GaveUp => return Err(SessionError::GaveUp),
            SessionState::Reconnecting { .. } => return Ok(SendOutcome::NotConnected),
            SessionState::Connected => {}
        }
        let (reply, response) = oneshot::channel();
        let started = Arc::new(AtomicBool::new(false));
        let command = SendCommand {
            started: started.clone(),
            generation,
            kind,
            payload: payload.to_vec(),
            reply,
        };
        tokio::time::timeout(timeout, async {
            self.sender
                .send(command)
                .await
                .map_err(|_| SessionError::NotConnected)?;
            response.await.map_err(|_| SessionError::NotConnected)?
        })
        .await
        .map_err(|_| {
            SessionError::Transport(TransportError::Timeout(if started.load(Ordering::SeqCst) {
                "write"
            } else {
                "queue"
            }))
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn jitter_source_and_session_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Box<dyn JitterSource>>();
        assert_send::<ReconnectingSession<FakeConnector, SplitMix64>>();
        assert_send::<SessionReader<FakeConnector, SplitMix64>>();
        assert_send::<SessionWriter>();
    }

    #[tokio::test(start_paused = true)]
    async fn send_proceeds_while_recv_in_backoff() {
        let (mut session, calls) = session(vec![transport(&Sent::default(), vec![], false)], 10);
        session.connect_initial().await.unwrap();
        let (mut reader, mut writer) = session.split();
        let reader_task = tokio::spawn(async move { reader.recv().await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(*calls.lock().unwrap(), 2); // First reconnect failed; second sleep pending.
        let start = tokio::time::Instant::now();
        let writer_task = tokio::spawn(async move {
            let result = writer.send(1, b"no queue", Duration::from_secs(1)).await;
            (writer, result)
        });
        let (mut writer, result) = writer_task.await.unwrap();
        assert_eq!(result.unwrap(), SendOutcome::NotConnected);
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert!(matches!(
            writer.state(),
            SessionState::Reconnecting { attempt: 2 }
        ));
        assert!(writer
            .take_events()
            .iter()
            .any(|event| matches!(event, SessionEvent::ReconnectFailed { attempt: 1, .. })));
        reader_task.abort();
    }

    struct IdleTransport(Sent);
    #[async_trait]
    impl Transport for IdleTransport {
        async fn send(&mut self, kind: u8, payload: &[u8]) -> Result<(), TransportError> {
            self.0.lock().unwrap().push(Frame {
                kind,
                payload: payload.to_vec(),
            });
            Ok(())
        }
        async fn recv(&mut self) -> Result<Frame, TransportError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn writer_uses_replacement_transport() {
        let old = Sent::default();
        let new = Sent::default();
        let (mut session, _) = session(
            vec![
                transport(&old, vec![], false),
                Ok(Box::new(IdleTransport(new.clone()))),
            ],
            2,
        );
        session.connect_initial().await.unwrap();
        session.set_last_seq(42);
        let (mut reader, mut writer) = session.split();
        let task = tokio::spawn(async move { reader.recv().await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(writer.state(), SessionState::Connected);
        tokio::spawn(async move {
            assert_eq!(
                writer
                    .send(7, b"new", Duration::from_secs(1))
                    .await
                    .unwrap(),
                SendOutcome::Sent
            );
        })
        .await
        .unwrap();
        assert!(old.lock().unwrap().is_empty());
        assert_eq!(
            *new.lock().unwrap(),
            vec![
                Frame {
                    kind: 9,
                    payload: Message::Resume {
                        session_id: 123,
                        last_seq: 42
                    }
                    .to_bytes()
                    .unwrap()
                },
                Frame {
                    kind: 7,
                    payload: b"new".to_vec()
                }
            ]
        );
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn split_cancelled_send_drops_transport() {
        let sent = Sent::default();
        let (mut session, calls) = session(
            vec![
                Ok(Box::new(BlockedTransport)),
                Ok(Box::new(IdleTransport(sent.clone()))),
            ],
            2,
        );
        session.connect_initial().await.unwrap();
        let (mut reader, mut writer) = session.split();
        let task = tokio::spawn(async move { reader.recv().await });
        assert!(matches!(
            writer.send(1, b"partial", Duration::from_millis(1)).await,
            Err(SessionError::Transport(TransportError::Timeout("write")))
        ));
        tokio::task::yield_now().await;
        assert!(matches!(writer.state(), SessionState::Reconnecting { .. }));
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(writer.state(), SessionState::Connected);
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(sent.lock().unwrap()[0].kind, 9);
        task.abort();
    }

    struct BlockedTransport;
    #[async_trait]
    impl Transport for BlockedTransport {
        async fn send(&mut self, _: u8, _: &[u8]) -> Result<(), TransportError> {
            std::future::pending().await
        }
        async fn recv(&mut self) -> Result<Frame, TransportError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_recv_during_inline_write_disconnects_and_recovers() {
        let sent = Sent::default();
        let (mut session, calls) = session(
            vec![
                Ok(Box::new(BlockedTransport)),
                Ok(Box::new(IdleTransport(sent.clone()))),
            ],
            2,
        );
        session.connect_initial().await.unwrap();
        let (mut reader, mut writer) = session.split();
        let writer_task = tokio::spawn(async move {
            let result = writer.send(1, b"partial", Duration::from_secs(10)).await;
            (writer, result)
        });
        tokio::task::yield_now().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(1), reader.recv())
                .await
                .is_err()
        );
        assert!(reader.session.current.is_none());
        assert_eq!(reader.state(), SessionState::Reconnecting { attempt: 0 });
        let (mut writer, result) = writer_task.await.unwrap();
        assert!(matches!(result, Err(SessionError::NotConnected)));
        assert_eq!(writer.state(), reader.state());
        assert_eq!(
            writer.take_events(),
            vec![
                SessionEvent::Connected,
                SessionEvent::Disconnected {
                    reason: TransportError::Closed.to_string()
                },
            ]
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(101), reader.recv())
                .await
                .is_err()
        );
        assert_eq!(writer.state(), SessionState::Connected);
        assert_eq!(*calls.lock().unwrap(), 2);
        let frames = sent.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            Message::from_frame_parts(frames[0].kind, &frames[0].payload).unwrap(),
            Message::Resume {
                session_id: 123,
                last_seq: 0
            }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn queue_timeouts_do_not_disconnect_or_write() {
        let sent = Sent::default();
        let (mut session, _) = session(vec![Ok(Box::new(IdleTransport(sent.clone())))], 2);
        session.connect_initial().await.unwrap();
        let (mut reader, mut writer) = session.split();
        // First timeout leaves a cancelled command in the one-slot queue;
        // the second must time out waiting for capacity.
        for _ in 0..2 {
            assert!(matches!(
                writer.send(1, b"queued", Duration::from_millis(1)).await,
                Err(SessionError::Transport(TransportError::Timeout("queue")))
            ));
            assert_eq!(writer.state(), SessionState::Connected);
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(1), reader.recv())
                .await
                .is_err()
        );
        assert!(sent.lock().unwrap().is_empty());
        assert_eq!(writer.take_events(), vec![SessionEvent::Connected]);
    }

    #[test]
    fn resume_payload_roundtrip_and_bad_length() {
        let request = Message::Resume {
            session_id: 0x0102030405060708,
            last_seq: u64::MAX,
        };
        let (kind, payload) = request.to_frame_parts().unwrap();
        assert_eq!(kind, 9);
        assert_eq!(&payload[4..12], &[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(Message::from_frame_parts(kind, &payload).unwrap(), request);
        for length in [0, 8, 15, 17, 32] {
            assert!(Message::from_frame_parts(kind, &vec![0; length]).is_err());
        }
    }
    #[test]
    fn backoff_grows_and_caps() {
        let policy = BackoffPolicy::default()
            .with_initial(Duration::from_secs(1))
            .with_jitter(0.0)
            .with_max_attempts(Some(5));
        let mut backoff = Backoff::new(policy, SplitMix64::new(1));
        for seconds in [1, 2, 4, 5, 5] {
            assert_eq!(backoff.next(), Some(Duration::from_secs(seconds)));
        }
        assert_eq!(backoff.next(), None);
        backoff.reset();
        assert_eq!(backoff.next(), Some(Duration::from_secs(1)));
    }
    #[test]
    fn backoff_jitter_within_bounds() {
        let policy = BackoffPolicy::default()
            .with_initial(Duration::from_secs(2))
            .with_multiplier(1.0)
            .with_jitter(0.5);
        let delays: Vec<_> = Backoff::new(policy.clone(), SplitMix64::new(42))
            .take(100)
            .collect();
        assert_eq!(
            delays,
            Backoff::new(policy, SplitMix64::new(42))
                .take(100)
                .collect::<Vec<_>>()
        );
        assert!(delays
            .iter()
            .all(|d| *d >= Duration::from_secs(1) && *d <= Duration::from_secs(3)));
        assert!(delays.windows(2).any(|w| w[0] != w[1]));
        let policy = BackoffPolicy::default()
            .with_initial(Duration::from_secs(5))
            .with_jitter(1.0);
        assert!(Backoff::new(policy, SplitMix64::new(3))
            .take(100)
            .all(|d| d <= Duration::from_secs(5)));
    }

    #[test]
    fn backoff_never_returns_zero() {
        struct Zero;
        impl JitterSource for Zero {
            fn next_unit(&mut self) -> f64 {
                0.0
            }
        }
        for policy in [
            BackoffPolicy::default().with_initial(Duration::ZERO),
            BackoffPolicy::default().with_jitter(1.0),
            BackoffPolicy::default().with_max(Duration::ZERO),
        ] {
            assert!(Backoff::new(policy, Zero)
                .take(10)
                .all(|delay| delay >= Duration::from_millis(1)));
        }
    }

    #[tokio::test]
    async fn oversized_send_keeps_connection() {
        let sent = Sent::default();
        let (mut session, calls) = session(vec![transport(&sent, vec![], false)], 2);
        session.connect_initial().await.unwrap();
        assert!(matches!(
            session.send(1, &vec![0; MAX_PAYLOAD_LEN + 1]).await,
            Err(SessionError::Transport(TransportError::Frame(
                FrameError::TooLarge { .. }
            )))
        ));
        assert_eq!(session.state(), SessionState::Connected);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert!(sent.lock().unwrap().is_empty());
        assert_eq!(session.send(1, b"ok").await.unwrap(), SendOutcome::Sent);
        assert_eq!(session.take_events(), vec![SessionEvent::Connected]);
    }

    struct SendErrorTransport(Option<TransportError>);
    #[async_trait]
    impl Transport for SendErrorTransport {
        async fn send(&mut self, _: u8, _: &[u8]) -> Result<(), TransportError> {
            match self.0.take() {
                Some(error) => Err(error),
                None => std::future::pending().await,
            }
        }
        async fn recv(&mut self) -> Result<Frame, TransportError> {
            Err(TransportError::Closed)
        }
    }

    #[tokio::test]
    async fn local_send_errors_keep_transport() {
        for error in [
            TransportError::Frame(FrameError::Truncated),
            TransportError::Timeout("read"),
            TransportError::Closed,
        ] {
            let (mut session, calls) =
                session(vec![Ok(Box::new(SendErrorTransport(Some(error))))], 2);
            session.connect_initial().await.unwrap();
            assert!(matches!(
                session.send(1, b"x").await,
                Err(SessionError::Transport(_))
            ));
            assert_eq!(session.state(), SessionState::Connected);
            assert!(session.current.is_some());
            assert_eq!(*calls.lock().unwrap(), 1);
            assert_eq!(session.take_events(), vec![SessionEvent::Connected]);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_send_drops_transport_and_recovers() {
        let sent = Sent::default();
        let (mut session, calls) = session(
            vec![
                Ok(Box::new(SendErrorTransport(None))),
                transport(&sent, vec![], false),
            ],
            2,
        );
        session.connect_initial().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(1), session.send(1, b"partial"))
                .await
                .is_err()
        );
        assert!(session.current.is_none());
        assert_eq!(session.state(), SessionState::Reconnecting { attempt: 0 });
        assert_eq!(
            session.send(1, b"retry").await.unwrap(),
            SendOutcome::ReconnectedNotSent
        );
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(sent.lock().unwrap()[0].kind, 9);
    }

    #[tokio::test(start_paused = true)]
    async fn received_frame_resets_reconnect_budget() {
        let sent = Sent::default();
        let frame = Frame {
            kind: 1,
            payload: vec![],
        };
        let (mut session, calls) = session(
            vec![
                transport(&sent, vec![], false),
                transport(&sent, vec![Ok(frame.clone())], false),
                transport(&sent, vec![Ok(frame.clone())], false),
            ],
            1,
        );
        session.connect_initial().await.unwrap();
        assert_eq!(session.recv().await.unwrap(), frame);
        assert_eq!(session.backoff.attempts, 0);
        assert_eq!(session.recv().await.unwrap(), frame);
        assert_eq!(session.backoff.attempts, 0);
        assert_eq!(*calls.lock().unwrap(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn resume_without_received_frame_preserves_budget() {
        let sent = Sent::default();
        let (mut session, calls) = session(
            vec![
                transport(&sent, vec![], false),
                transport(&sent, vec![], false),
                transport(&sent, vec![], false),
            ],
            2,
        );
        session.connect_initial().await.unwrap();
        assert!(matches!(session.recv().await, Err(SessionError::GaveUp)));
        assert_eq!(*calls.lock().unwrap(), 3);
        assert_eq!(session.backoff.attempts, 2);
    }

    type Sent = Arc<Mutex<Vec<Frame>>>;
    struct FakeTransport {
        sent: Sent,
        reads: VecDeque<Result<Frame, TransportError>>,
        fail_send: bool,
    }
    #[async_trait]
    impl Transport for FakeTransport {
        async fn send(&mut self, kind: u8, payload: &[u8]) -> Result<(), TransportError> {
            if self.fail_send {
                return Err(TransportError::Timeout("write"));
            }
            self.sent.lock().unwrap().push(Frame {
                kind,
                payload: payload.to_vec(),
            });
            Ok(())
        }
        async fn recv(&mut self) -> Result<Frame, TransportError> {
            self.reads
                .pop_front()
                .unwrap_or(Err(TransportError::Closed))
        }
    }
    struct FakeConnector {
        results: VecDeque<Result<Box<dyn Transport>, TransportError>>,
        calls: Arc<Mutex<u32>>,
    }
    #[async_trait]
    impl Connector for FakeConnector {
        async fn connect(&mut self) -> Result<Box<dyn Transport>, TransportError> {
            *self.calls.lock().unwrap() += 1;
            self.results
                .pop_front()
                .unwrap_or(Err(TransportError::Closed))
        }
    }
    fn transport(
        sent: &Sent,
        reads: Vec<Result<Frame, TransportError>>,
        fail_send: bool,
    ) -> Result<Box<dyn Transport>, TransportError> {
        Ok(Box::new(FakeTransport {
            sent: sent.clone(),
            reads: reads.into(),
            fail_send,
        }))
    }
    fn session(
        results: Vec<Result<Box<dyn Transport>, TransportError>>,
        max: u32,
    ) -> (
        ReconnectingSession<FakeConnector, SplitMix64>,
        Arc<Mutex<u32>>,
    ) {
        let calls = Arc::new(Mutex::new(0));
        let connector = FakeConnector {
            results: results.into(),
            calls: calls.clone(),
        };
        (
            ReconnectingSession::new(
                connector,
                123,
                BackoffPolicy::default()
                    .with_jitter(0.0)
                    .with_max_attempts(Some(max)),
                SplitMix64::new(42),
            ),
            calls,
        )
    }
    #[tokio::test(start_paused = true)]
    async fn reconnects_after_transport_drop() {
        let sent = Sent::default();
        let expected = Frame {
            kind: 3,
            payload: vec![43],
        };
        let (mut session, calls) = session(
            vec![
                transport(&sent, vec![], false),
                Err(TransportError::Closed),
                Err(TransportError::Closed),
                transport(&sent, vec![Ok(expected.clone())], false),
            ],
            3,
        );
        session.connect_initial().await.unwrap();
        session.set_last_seq(42);
        let start = tokio::time::Instant::now();
        assert_eq!(session.recv().await.unwrap(), expected);
        assert_eq!(start.elapsed(), Duration::from_millis(700));
        assert_eq!(*calls.lock().unwrap(), 4);
        assert_eq!(
            *sent.lock().unwrap(),
            vec![Frame {
                kind: 9,
                payload: Message::Resume {
                    session_id: 123,
                    last_seq: 42
                }
                .to_bytes()
                .unwrap()
            }]
        );
        assert_eq!(session.state(), SessionState::Connected);
        assert_eq!(
            session.take_events(),
            vec![
                SessionEvent::Connected,
                SessionEvent::Disconnected {
                    reason: TransportError::Closed.to_string()
                },
                SessionEvent::ReconnectAttempt {
                    attempt: 1,
                    delay: Duration::from_millis(100)
                },
                SessionEvent::ReconnectFailed {
                    attempt: 1,
                    error: TransportError::Closed.to_string()
                },
                SessionEvent::ReconnectAttempt {
                    attempt: 2,
                    delay: Duration::from_millis(200)
                },
                SessionEvent::ReconnectFailed {
                    attempt: 2,
                    error: TransportError::Closed.to_string()
                },
                SessionEvent::ReconnectAttempt {
                    attempt: 3,
                    delay: Duration::from_millis(400)
                },
                SessionEvent::Connected,
                SessionEvent::Resumed { last_seq: 42 },
            ]
        );
        assert!(session.take_events().is_empty());
    }
    #[tokio::test(start_paused = true)]
    async fn gives_up_after_max_attempts() {
        let (mut session, calls) = session(vec![transport(&Sent::default(), vec![], false)], 2);
        session.connect_initial().await.unwrap();
        assert!(matches!(session.recv().await, Err(SessionError::GaveUp)));
        assert_eq!(session.state(), SessionState::GaveUp);
        assert!(matches!(
            session.send(1, b"x").await,
            Err(SessionError::GaveUp)
        ));
        assert!(matches!(session.recv().await, Err(SessionError::GaveUp)));
        assert!(matches!(
            session.connect_initial().await,
            Err(SessionError::GaveUp)
        ));
        assert_eq!(*calls.lock().unwrap(), 3);
        assert_eq!(
            session.take_events().last(),
            Some(&SessionEvent::GaveUp { attempts: 2 })
        );
    }
    #[tokio::test(start_paused = true)]
    async fn read_timeout_keeps_connection() {
        let expected = Frame {
            kind: 2,
            payload: vec![1],
        };
        let (mut session, calls) = session(
            vec![transport(
                &Sent::default(),
                vec![Err(TransportError::Timeout("read")), Ok(expected.clone())],
                false,
            )],
            2,
        );
        session.connect_initial().await.unwrap();
        assert!(matches!(
            session.recv().await,
            Err(SessionError::Transport(TransportError::Timeout("read")))
        ));
        assert_eq!(session.state(), SessionState::Connected);
        assert_eq!(session.recv().await.unwrap(), expected);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(session.take_events(), vec![SessionEvent::Connected]);
    }
    #[tokio::test(start_paused = true)]
    async fn write_timeout_and_failed_resume_do_not_replay() {
        let sent = Sent::default();
        let (mut session, calls) = session(
            vec![
                transport(&sent, vec![], true),
                transport(&sent, vec![], true),
                transport(&sent, vec![], false),
            ],
            2,
        );
        session.connect_initial().await.unwrap();
        session.set_last_seq(9);
        assert_eq!(
            session.send(5, b"application").await.unwrap(),
            SendOutcome::ReconnectedNotSent
        );
        assert_eq!(*calls.lock().unwrap(), 3);
        assert_eq!(
            *sent.lock().unwrap(),
            vec![Frame {
                kind: 9,
                payload: Message::Resume {
                    session_id: 123,
                    last_seq: 9
                }
                .to_bytes()
                .unwrap()
            }]
        );
        assert_eq!(
            session.send(5, b"application").await.unwrap(),
            SendOutcome::Sent
        );
        assert_eq!(sent.lock().unwrap().len(), 2);
        assert!(session
            .take_events()
            .iter()
            .any(|e| matches!(e, SessionEvent::ReconnectFailed { attempt: 1, .. })));
    }
    #[tokio::test(start_paused = true)]
    async fn zero_budget_and_bounded_events() {
        let (mut session, calls) = session(vec![transport(&Sent::default(), vec![], false)], 0);
        assert!(matches!(
            session.recv().await,
            Err(SessionError::NotConnected)
        ));
        session.connect_initial().await.unwrap();
        assert!(matches!(session.recv().await, Err(SessionError::GaveUp)));
        assert_eq!(*calls.lock().unwrap(), 1);
        for _ in 0..100 {
            session.emit(SessionEvent::Connected);
        }
        assert_eq!(session.take_events().len(), 64);
    }
}
