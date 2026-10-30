//! Latest-frame-wins raw capture slot; encoding happens only on writer demand.
use crate::Error;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;
use trinix_core::{Bytes, Capture, Encoder, MonotonicClock};

/// Adds fallible encoding, keyframe control and accurate metadata to core Encoder.
///
/// Implementations must honor `force` by emitting an independently decodable
/// keyframe and report the actual encoded frame type. The core trait alone does
/// not expose keyframe control.
pub trait HostEncoder: Encoder + Send + 'static {
    fn encode_video(
        &mut self,
        raw: &[u8],
        timestamp_us: u64,
        force: bool,
    ) -> Result<(Bytes, bool), Error>;
}
/// Age is measured at send initiation; network delivery time is peer-controlled.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub queue_len: usize,
    pub max_queue_len: usize,
    pub captured: u64,
    pub dropped: u64,
    pub sent: u64,
    pub max_send_age: Duration,
}
struct Raw {
    data: Bytes,
    timestamp_us: u64,
    captured: Instant,
}
#[derive(Default)]
struct Slot {
    frame: Option<Raw>,
    closed: bool,
    metrics: Metrics,
}
impl Slot {
    fn push(&mut self, frame: Raw) {
        if self.frame.replace(frame).is_some() {
            self.metrics.dropped += 1;
        }
        self.metrics.captured += 1;
        self.metrics.queue_len = 1;
        self.metrics.max_queue_len = 1;
    }
    fn take(&mut self) -> Option<Raw> {
        self.metrics.queue_len = 0;
        self.frame.take()
    }
}
// Closing on both normal return and unwind prevents a panicking backend from
// leaving next() asleep forever. This also recovers any poisoned slot lock.
struct CaptureGuard(Arc<Mutex<Slot>>, Arc<Notify>);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        self.1.notify_one();
    }
}
/// A snapshot handle retains no additional frames.
#[derive(Clone)]
pub struct Monitor(Arc<Mutex<Slot>>);
impl Monitor {
    pub fn snapshot(&self) -> Metrics {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .metrics
            .clone()
    }
}
/// Capture runs on a dedicated thread; dropping signals it to stop after its
/// current backend call. Encoding runs on the blocking pool, outside Tokio.
pub struct Pipeline<E> {
    slot: Arc<Mutex<Slot>>,
    ready: Arc<Notify>,
    encoder: Option<E>,
    interval: Duration,
    abr: crate::abr::Abr,
    abr_clock: Instant,
    blocked: bool,
}
pub struct Video {
    pub data: Bytes,
    pub timestamp_us: u64,
    pub keyframe: bool,
    pub captured: Instant,
}
impl<E: HostEncoder> Pipeline<E> {
    pub fn new<C: Capture + Send + 'static>(
        mut capture: C,
        encoder: E,
        fps: u32,
    ) -> Result<Self, Error> {
        if !(1..=240).contains(&fps) {
            return Err("fps must be in 1..=240".into());
        }
        let interval = Duration::from_secs_f64(1.0 / f64::from(fps));
        let slot = Arc::new(Mutex::new(Slot::default()));
        let ready = Arc::new(Notify::new());
        let shared = slot.clone();
        let notify = ready.clone();
        std::thread::Builder::new()
            .name("mm-capture".into())
            .spawn(move || {
                let _guard = CaptureGuard(shared.clone(), notify.clone());
                let clock = MonotonicClock::new();
                loop {
                    let start = Instant::now();
                    if shared.lock().unwrap_or_else(|e| e.into_inner()).closed {
                        break;
                    }
                    let timestamp_us = clock.now_us();
                    let Some(data) = capture.next_frame() else {
                        shared.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
                        notify.notify_one();
                        break;
                    };
                    let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                    if state.closed {
                        break;
                    }
                    state.push(Raw {
                        data,
                        timestamp_us,
                        captured: start,
                    });
                    drop(state);
                    notify.notify_one();
                    std::thread::sleep(interval.saturating_sub(start.elapsed()));
                }
            })?;
        Ok(Self {
            slot,
            ready,
            encoder: Some(encoder),
            interval,
            abr: crate::abr::Abr::new(4_000, 500, 20_000)?,
            abr_clock: Instant::now(),
            blocked: false,
        })
    }
    /// Configure the ABR starting point to match the encoder's CLI bitrate.
    pub fn with_bitrate(mut self, kbps: u32) -> Result<Self, Error> {
        self.abr = crate::abr::Abr::new(kbps, kbps.min(500), kbps.max(20_000))?;
        self.encoder
            .as_mut()
            .ok_or("encoder unavailable")?
            .set_bitrate_kbps(kbps)
            .map_err(|e| format!("set initial bitrate: {e:?}"))?;
        Ok(self)
    }
    /// Return a rejected bitrate update to the caller; the encoder retains its
    /// previous bitrate, so a live session can log the error and continue.
    pub fn on_stats(&mut self, stats: &trinix_core::NetStats) -> Result<(), Error> {
        self.abr
            .apply(
                self.encoder.as_mut().ok_or("encoder unavailable")?,
                stats,
                self.abr_clock.elapsed(),
                self.blocked,
            )
            .map_err(|e| format!("bitrate update rejected: {e:?}"))?;
        self.blocked = false;
        Ok(())
    }
    pub(crate) fn write_completed(&mut self, elapsed: Duration) {
        self.blocked |= elapsed > Duration::from_millis(100);
    }
    pub fn monitor(&self) -> Monitor {
        Monitor(self.slot.clone())
    }
    pub async fn next(&mut self, mut force: bool) -> Result<Option<Video>, Error> {
        loop {
            let notified = self.ready.notified();
            let (raw, closed) = {
                let mut state = self.slot.lock().unwrap_or_else(|e| e.into_inner());
                (state.take(), state.closed)
            };
            let Some(raw) = raw else {
                if closed {
                    return Ok(None);
                }
                notified.await;
                continue;
            };
            if raw.captured.elapsed() >= self.interval * 2 {
                self.slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .metrics
                    .dropped += 1;
                continue;
            }
            let mut encoder = self.encoder.take().ok_or("encoder unavailable")?;
            let (returned, result) = tokio::task::spawn_blocking(move || {
                let result = encoder
                    .encode_video(&raw.data, raw.timestamp_us, force)
                    .map(|(data, keyframe)| Video {
                        data,
                        keyframe,
                        timestamp_us: raw.timestamp_us,
                        captured: raw.captured,
                    });
                (encoder, result)
            })
            .await?;
            self.encoder = Some(returned);
            let video = result?;
            if video.data.is_empty() {
                return Err("encoder returned empty frame".into());
            }
            if video.captured.elapsed() >= self.interval * 2 {
                self.slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .metrics
                    .dropped += 1;
                force = true;
                continue;
            }
            return Ok(Some(video));
        }
    }
    pub(crate) fn sending(&self, captured: Instant) -> bool {
        let mut state = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        let age = captured.elapsed();
        if age >= self.interval * 2 {
            state.metrics.dropped += 1;
            return false;
        }
        state.metrics.max_send_age = state.metrics.max_send_age.max(age);
        true
    }
    pub(crate) fn sent(&self) {
        self.slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .metrics
            .sent += 1;
    }
}
impl<E> Drop for Pipeline<E> {
    fn drop(&mut self) {
        let mut state = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        state.frame = None;
        state.metrics.queue_len = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_frame_wins() {
        let mut slot = Slot::default();
        for n in 0..100 {
            slot.push(Raw {
                data: vec![n],
                timestamp_us: n.into(),
                captured: Instant::now(),
            });
            assert_eq!(slot.metrics.queue_len, 1);
        }
        assert_eq!(slot.metrics.dropped, 99);
        assert_eq!(slot.take().unwrap().data, [99]);
        assert!(slot.take().is_none());
        assert_eq!(slot.metrics.queue_len, 0);
        assert_eq!(slot.metrics.max_queue_len, 1);
    }

    #[test]
    fn taking_then_refilling_does_not_count_a_drop() {
        let mut slot = Slot::default();
        for timestamp_us in 0..3 {
            slot.push(Raw {
                data: vec![],
                timestamp_us,
                captured: Instant::now(),
            });
            assert_eq!(slot.take().unwrap().timestamp_us, timestamp_us);
            assert_eq!(slot.metrics.queue_len, 0);
        }
        assert_eq!(slot.metrics.captured, 3);
        assert_eq!(slot.metrics.dropped, 0);
        assert_eq!(slot.metrics.max_queue_len, 1);
    }
}
