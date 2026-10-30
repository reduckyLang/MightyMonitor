//! One authorized capture session, reused across sequential network viewers.
use crate::CapturedFrame;
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use trinix_core::{Bytes, Capture};
#[derive(Default)]
struct Latest {
    frame: Option<CapturedFrame>,
    closed: bool,
}
pub struct SharedCapture {
    latest: Arc<Mutex<Latest>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl SharedCapture {
    /// The source must poll within the supplied timeout. None means an idle
    /// desktop, not a closed portal; errors mean the capture session ended.
    pub fn start(
        mut source: impl FnMut(Duration) -> io::Result<Option<CapturedFrame>> + Send + 'static,
    ) -> io::Result<Self> {
        let latest = Arc::new(Mutex::new(Latest::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let state = latest.clone();
        let signal = stop.clone();
        let worker = thread::Builder::new()
            .name("mm-shared-capture".into())
            .spawn(move || {
                // Also close readers if a platform source panics.
                struct Close(Arc<Mutex<Latest>>);
                impl Drop for Close {
                    fn drop(&mut self) {
                        self.0.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
                    }
                }
                let _close = Close(state.clone());
                while !signal.load(Ordering::Relaxed) {
                    match source(Duration::from_millis(100)) {
                        Ok(Some(frame)) => {
                            state.lock().unwrap_or_else(|e| e.into_inner()).frame = Some(frame)
                        }
                        Ok(None) => {}
                        Err(_) => break,
                    }
                }
            })?;
        let capture = Self {
            latest,
            stop,
            worker: Some(worker),
        };
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let state = capture.latest.lock().unwrap_or_else(|e| e.into_inner());
            if state.frame.is_some() {
                drop(state);
                return Ok(capture);
            }
            if state.closed || Instant::now() >= end {
                drop(state);
                return Err(io::Error::other(
                    "No screen frames received from the selected capture source",
                ));
            }
            drop(state);
            thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn viewer(&self) -> io::Result<(SharedViewer, u32, u32)> {
        let latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if latest.closed {
            return Err(io::Error::other(
                "Screen sharing permission ended. Stop sharing and start again.",
            ));
        }
        let frame = latest
            .frame
            .as_ref()
            .ok_or_else(|| io::Error::other("Screen capture is not ready"))?;
        let (width, height) = (frame.width & !1, frame.height & !1);
        if width < 2 || height < 2 || width > 8192 || height > 8192 {
            return Err(io::Error::other("Unsupported selected screen dimensions"));
        }
        Ok((
            SharedViewer {
                latest: self.latest.clone(),
                width,
                height,
                original_width: frame.width,
                original_height: frame.height,
            },
            width,
            height,
        ))
    }
}
impl Drop for SharedCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub struct SharedViewer {
    latest: Arc<Mutex<Latest>>,
    width: u32,
    height: u32,
    original_width: u32,
    original_height: u32,
}
impl Capture for SharedViewer {
    fn next_frame(&mut self) -> Option<Bytes> {
        let state = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return None;
        }
        let frame = state.frame.as_ref()?;
        // Close this network session on a size change; reconnect renegotiates
        // dimensions from the same portal, without asking for consent again.
        if (frame.width, frame.height) != (self.original_width, self.original_height) {
            return None;
        }
        if frame.data.len()
            != (frame.width as usize)
                .checked_mul(frame.height as usize)?
                .checked_mul(4)?
        {
            return None;
        }
        let mut rgba = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for row in frame
            .data
            .chunks_exact(frame.width as usize * 4)
            .take(self.height as usize)
        {
            for p in row[..self.width as usize * 4].chunks_exact(4) {
                rgba.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        Some(rgba)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    fn frame(w: u32, h: u32) -> CapturedFrame {
        CapturedFrame {
            width: w,
            height: h,
            timestamp_us: 0,
            data: [1, 2, 3, 255].repeat((w * h) as usize),
        }
    }
    #[test]
    fn static_screen_reuses_latest_frame_and_reconnect_renegotiates_resize() {
        let (tx, rx) = mpsc::channel();
        tx.send(frame(5, 3)).unwrap();
        let source = SharedCapture::start(move |timeout| match rx.recv_timeout(timeout) {
            Ok(f) => Ok(Some(f)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(_) => Err(io::ErrorKind::UnexpectedEof.into()),
        })
        .unwrap();
        let (mut first, w, h) = source.viewer().unwrap();
        assert_eq!((w, h), (4, 2));
        let expected = [3, 2, 1, 255].repeat(8);
        assert_eq!(first.next_frame(), Some(expected.clone()));
        thread::sleep(Duration::from_millis(150));
        assert_eq!(first.next_frame(), Some(expected));
        tx.send(frame(8, 6)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while first.next_frame().is_some() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let (mut second, w, h) = source.viewer().unwrap();
        assert_eq!((w, h), (8, 6));
        assert_eq!(second.next_frame().unwrap().len(), 8 * 6 * 4);
        drop(source);
        assert!(second.next_frame().is_none());
    }
    #[test]
    fn closed_source_does_not_keep_showing_a_stale_screen() {
        let mut first = true;
        let source = SharedCapture::start(move |_| {
            if first {
                first = false;
                Ok(Some(frame(2, 2)))
            } else {
                thread::sleep(Duration::from_millis(50));
                Err(io::ErrorKind::UnexpectedEof.into())
            }
        })
        .unwrap();
        let (mut viewer, _, _) = source.viewer().unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(viewer.next_frame().is_none());
        assert!(source.viewer().is_err());
    }
}
