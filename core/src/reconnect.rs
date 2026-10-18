//! Frame-level reconnect, explicit resume, and deterministic backoff.

use crate::{Frame, TcpTransport, Transport, TransportError};
use async_trait::async_trait;
use std::{collections::VecDeque, time::Duration};

/// Provisional Resume kind; map to `Message::Resume` when M1-3 lands.
pub const KIND_RESUME: u8 = 0xfe;

/// Resume position supplied by the application, not inferred from opaque frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeRequest {
    /// Existing logical session identifier.
    pub session_id: u64,
    /// Last sequence successfully consumed by the application.
    pub last_seq: u64,
}

/// Resume payload did not contain exactly 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("resume payload must be 16 bytes (received {0})")]
pub struct ResumePayloadError(pub usize);

impl ResumeRequest {
    /// Encode session ID followed by last sequence, both little-endian.
    pub fn to_payload(self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.session_id.to_le_bytes());
        bytes[8..].copy_from_slice(&self.last_seq.to_le_bytes());
        bytes
    }
    /// Decode an exact 16-byte payload.
    pub fn from_payload(bytes: &[u8]) -> Result<Self, ResumePayloadError> {
        let bytes: &[u8; 16] = bytes
            .try_into()
            .map_err(|_| ResumePayloadError(bytes.len()))?;
        let mut id = [0; 8];
        let mut seq = [0; 8];
        id.copy_from_slice(&bytes[..8]);
        seq.copy_from_slice(&bytes[8..]);
        Ok(Self {
            session_id: u64::from_le_bytes(id),
            last_seq: u64::from_le_bytes(seq),
        })
    }
}

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
pub trait JitterSource {
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
    /// Set the hard delay cap, including jitter.
    pub fn with_max(mut self, max: Duration) -> Self {
        self.max = max;
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
            .min(self.policy.max);
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
        /// Attempts performed during this outage.
        attempts: u32,
    },
}
/// Session operation failure.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
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
}

/// Reconnects frame transports and sends Resume first on every replacement.
///
/// Callers maintain the last consumed sequence. Receives transparently recover;
/// sends report recovery without replay. Canceling a send can leave partial output:
/// drop the session in that case. Recovery cancellation retains its retry budget.
pub struct ReconnectingSession<C: Connector, J: JitterSource> {
    connector: C,
    backoff: Backoff<J>,
    session_id: u64,
    last_seq: u64,
    current: Option<Box<dyn Transport>>,
    state: SessionState,
    started: bool,
    events: VecDeque<SessionEvent>,
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
        }
    }
    /// Set the last sequence consumed by the caller, to request replay after it.
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
        self.backoff.reset();
        self.state = SessionState::Reconnecting { attempt: 0 };
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
                transport
                    .send(
                        KIND_RESUME,
                        &ResumeRequest {
                            session_id: self.session_id,
                            last_seq: self.last_seq,
                        }
                        .to_payload(),
                    )
                    .await?;
                Ok::<_, TransportError>(transport)
            }
            .await;
            match result {
                Ok(transport) => {
                    self.current = Some(transport);
                    self.state = SessionState::Connected;
                    self.backoff.reset();
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
        let recovering = self.started && self.current.is_none();
        self.ready().await?;
        if recovering {
            return Ok(SendOutcome::ReconnectedNotSent);
        }
        let result = match self.current.as_mut() {
            Some(transport) => transport.send(kind, payload).await,
            None => return Err(SessionError::NotConnected),
        };
        match result {
            Ok(()) => Ok(SendOutcome::Sent),
            Err(TransportError::Timeout(op)) if op != "write" => {
                Err(TransportError::Timeout(op).into())
            }
            Err(error) => {
                self.disconnect(&error);
                self.recover().await?;
                Ok(SendOutcome::ReconnectedNotSent)
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
                Ok(frame) => return Ok(frame),
                Err(error @ TransportError::Timeout("read")) => return Err(error.into()),
                Err(error) => {
                    self.disconnect(&error);
                    self.recover().await?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn resume_payload_roundtrip_and_bad_length() {
        let request = ResumeRequest {
            session_id: 0x0102030405060708,
            last_seq: u64::MAX,
        };
        assert_eq!(&request.to_payload()[..8], &[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(
            ResumeRequest::from_payload(&request.to_payload()),
            Ok(request)
        );
        for length in [0, 8, 15, 17, 32] {
            assert_eq!(
                ResumeRequest::from_payload(&vec![0; length]),
                Err(ResumePayloadError(length))
            );
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
                kind: KIND_RESUME,
                payload: [123u64.to_le_bytes(), 42u64.to_le_bytes()].concat()
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
                kind: KIND_RESUME,
                payload: ResumeRequest {
                    session_id: 123,
                    last_seq: 9
                }
                .to_payload()
                .to_vec()
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
