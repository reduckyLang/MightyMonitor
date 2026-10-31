//! Root-window GetImage capture, converted to tightly packed BGRA (opaque alpha).
use std::time::{Duration, Instant};

use thiserror::Error;
use trinix_core::{Bytes, Capture};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder, VisualClass},
    rust_connection::RustConnection,
};

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("X11 connection failed: {0}")]
    Connect(#[from] x11rb::errors::ConnectError),
    #[error("X11 request failed: {0}")]
    Connection(#[from] x11rb::errors::ConnectionError),
    #[error("X11 reply failed: {0}")]
    Reply(#[from] x11rb::errors::ReplyError),
    #[error(
        "unsupported X11 pixel layout (requires depth 24/32, 32 bits per pixel, TrueColor RGB8)"
    )]
    UnsupportedFormat,
    #[error("frame byte length does not match dimensions")]
    InvalidFrameLength,
}

/// Sources return exactly width * height * 4 bytes in BGRA order.
pub trait FrameSource {
    fn dimensions(&self) -> (u16, u16);
    fn grab(&mut self) -> Result<Vec<u8>, CaptureError>;
}

pub struct X11Capture {
    connection: RustConnection,
    root: u32,
    width: u16,
    height: u16,
    little_endian: bool,
    shifts: [u32; 3],
}

impl X11Capture {
    /// Connect to the selected display (None uses DISPLAY) and its default screen.
    pub fn open(display: Option<&str>) -> Result<Self, CaptureError> {
        let (connection, screen_index) = x11rb::connect(display)?;
        let setup = connection.setup();
        let screen = &setup.roots[screen_index];
        let format = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == screen.root_depth)
            .ok_or(CaptureError::UnsupportedFormat)?;
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| v.visual_id == screen.root_visual)
            .ok_or(CaptureError::UnsupportedFormat)?;
        let masks = [visual.red_mask, visual.green_mask, visual.blue_mask];
        if !matches!(screen.root_depth, 24 | 32)
            || format.bits_per_pixel != 32
            || !matches!(format.scanline_pad, 8 | 16 | 32)
            || visual.class != VisualClass::TRUE_COLOR
            || masks
                .iter()
                .any(|m| *m == 0 || (*m >> m.trailing_zeros()) != 255)
            || masks[0] & masks[1] != 0
            || masks[0] & masks[2] != 0
            || masks[1] & masks[2] != 0
        {
            return Err(CaptureError::UnsupportedFormat);
        }
        let result = Self {
            root: screen.root,
            width: screen.width_in_pixels,
            height: screen.height_in_pixels,
            little_endian: setup.image_byte_order == ImageOrder::LSB_FIRST,
            shifts: masks.map(u32::trailing_zeros),
            connection,
        };
        Ok(result)
    }

    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn height(&self) -> u16 {
        self.height
    }
}

impl FrameSource for X11Capture {
    fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn grab(&mut self) -> Result<Vec<u8>, CaptureError> {
        let mut data = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                0,
                0,
                self.width,
                self.height,
                u32::MAX,
            )?
            .reply()?
            .data;
        if data.len() != usize::from(self.width) * usize::from(self.height) * 4 {
            return Err(CaptureError::InvalidFrameLength);
        }
        // Common little-endian X11 layout is already BGR; normalize alpha only.
        if self.little_endian && self.shifts == [16, 8, 0] {
            let mut offset = 3;
            while offset < data.len() {
                data[offset] = 255;
                offset += 4;
            }
            return Ok(data);
        }
        for pixel in data.chunks_exact_mut(4) {
            let bytes = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let value = if self.little_endian {
                u32::from_le_bytes(bytes)
            } else {
                u32::from_be_bytes(bytes)
            };
            pixel.copy_from_slice(&[
                (value >> self.shifts[2]) as u8,
                (value >> self.shifts[1]) as u8,
                (value >> self.shifts[0]) as u8,
                255,
            ]);
        }
        Ok(data)
    }
}

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Microseconds since this capturer was created, measured with Instant.
    /// Recorded after the grab completes; not a wall-clock or server timestamp.
    pub timestamp_us: u64,
    pub data: Vec<u8>,
}

/// Capture backend wrapper with optional pacing. Errors map to None in the core API.
pub struct LinuxCapture<S: FrameSource> {
    source: S,
    start: Instant,
    interval: Option<Duration>,
    previous_grab: Option<Instant>,
    last_grab_duration: Duration,
}

impl<S: FrameSource> LinuxCapture<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            start: Instant::now(),
            interval: None,
            previous_grab: None,
            last_grab_duration: Duration::ZERO,
        }
    }

    /// Limit grab starts to this rate. Zero disables pacing; default is unpaced.
    pub fn with_fps(mut self, fps: u32) -> Self {
        self.interval = (fps != 0).then(|| Duration::from_secs_f64(1.0 / f64::from(fps)));
        self
    }

    pub fn dimensions(&self) -> (u16, u16) {
        self.source.dimensions()
    }

    /// Duration of the last grab, excluding any pacing sleep (including failed grabs).
    pub fn last_grab_duration(&self) -> Duration {
        self.last_grab_duration
    }

    /// Return the latest captured frame with metadata, transferring ownership to the caller.
    pub fn next_captured(&mut self) -> Option<CapturedFrame> {
        if let (Some(interval), Some(previous)) = (self.interval, self.previous_grab) {
            std::thread::sleep(interval.saturating_sub(previous.elapsed()));
        }
        let grab_start = Instant::now();
        self.previous_grab = Some(grab_start);
        let result = self.source.grab();
        self.last_grab_duration = grab_start.elapsed();
        let data = result.ok()?;
        let (width, height) = self.source.dimensions();
        if data.len() != usize::from(width) * usize::from(height) * 4 {
            return None;
        }
        Some(CapturedFrame {
            width: u32::from(width),
            height: u32::from(height),
            timestamp_us: self.start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            data,
        })
    }
}

impl<S: FrameSource> Capture for LinuxCapture<S> {
    fn next_frame(&mut self) -> Option<Bytes> {
        self.next_captured().map(|frame| frame.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSource {
        fail: bool,
        short: bool,
    }
    impl FrameSource for FakeSource {
        fn dimensions(&self) -> (u16, u16) {
            (3, 2)
        }
        fn grab(&mut self) -> Result<Vec<u8>, CaptureError> {
            if self.fail {
                Err(CaptureError::InvalidFrameLength)
            } else {
                Ok(vec![255; if self.short { 4 } else { 24 }])
            }
        }
    }
    fn fake() -> LinuxCapture<FakeSource> {
        LinuxCapture::new(FakeSource {
            fail: false,
            short: false,
        })
    }

    #[test]
    fn dimensions_bytes_and_timestamps() {
        let mut capture = fake();
        assert_eq!(capture.dimensions(), (3, 2));
        let first = capture.next_captured().unwrap();
        assert_eq!((first.width, first.height), (3, 2));
        assert_eq!(first.data.len(), 3 * 2 * 4);
        let second = capture.next_captured().unwrap();
        assert!(second.timestamp_us >= first.timestamp_us);
        assert_eq!(capture.next_frame().unwrap().len(), 24);
        assert!(capture.last_grab_duration() <= capture.start.elapsed());
    }

    #[test]
    fn failures_and_invalid_lengths_return_none() {
        let mut capture = LinuxCapture::new(FakeSource {
            fail: true,
            short: false,
        });
        assert!(capture.next_frame().is_none());
        let mut capture = LinuxCapture::new(FakeSource {
            fail: false,
            short: true,
        });
        assert!(capture.next_captured().is_none());
    }

    #[test]
    fn pacing_respects_interval() {
        let mut capture = fake().with_fps(50);
        let first = capture.next_captured().unwrap();
        for _ in 0..4 {
            capture.next_captured().unwrap();
        }
        let last = capture.next_captured().unwrap();
        assert!(last.timestamp_us - first.timestamp_us >= 90_000);
        assert!(last.timestamp_us - first.timestamp_us < 2_000_000);
        assert!(fake().interval.is_none());
        assert!(fake().with_fps(0).interval.is_none());
    }
}
