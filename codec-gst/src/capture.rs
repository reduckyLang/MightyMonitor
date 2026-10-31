//! Windows DXGI capture using GStreamer's native D3D11 source.
//! All frame conversion is safe Rust. The common path is testable on Linux.
use crate::{error, Result};
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::AppSink;
use gstreamer_video as video;

pub struct WindowsCapture {
    pipeline: gst::Pipeline,
    sink: AppSink,
    source_dimensions: (u32, u32),
    dimensions: (u32, u32),
    first: Option<Vec<u8>>,
}
impl WindowsCapture {
    /// Capture the primary monitor (-1) or a zero-based monitor index.
    /// Requires the Windows d3d11 plugin and an interactive desktop session.
    pub fn open(fps: u32, monitor: i32) -> Result<Self> {
        if !cfg!(target_os = "windows") {
            return Err(error("Windows desktop capture requires Windows"));
        }
        if !(1..=240).contains(&fps) || monitor < -1 {
            return Err(error("invalid capture fps or monitor index"));
        }
        // Only validated integers enter this pipeline description, never arbitrary
        // user-provided launch fragments. System-memory caps avoid GPU mapping.
        Self::from_description(&format!(
            "d3d11screencapturesrc capture-api=dxgi monitor-index={monitor} show-cursor=true ! video/x-raw,framerate={fps}/1 ! videoconvert ! video/x-raw,format=RGBA ! appsink name=capture sync=false max-buffers=1 drop=true"
        ))
    }
    fn from_description(description: &str) -> Result<Self> {
        gst::init().map_err(error)?;
        let pipeline = gst::parse::launch(description)
            .map_err(error)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| error("expected capture pipeline"))?;
        let sink = pipeline
            .by_name("capture")
            .ok_or_else(|| error("missing capture appsink"))?
            .downcast::<AppSink>()
            .map_err(|_| error("invalid capture appsink"))?;
        let mut capture = Self {
            pipeline,
            sink,
            source_dimensions: (0, 0),
            dimensions: (0, 0),
            first: None,
        };
        capture
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(error)?;
        let sample = capture.sample()?;
        let (rgba, source_dimensions, dimensions) = rgba(&sample)?;
        capture.source_dimensions = source_dimensions;
        capture.dimensions = dimensions;
        capture.first = Some(rgba);
        Ok(capture)
    }
    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }
    fn sample(&self) -> Result<gst::Sample> {
        self.sink.try_pull_sample(gst::ClockTime::from_seconds(3)).ok_or_else(|| {
            let detail = self.pipeline.bus()
                .and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]))
                .map(|message| format!("{message:?}"));
            error(detail.unwrap_or_else(|| "desktop capture ended or timed out; check desktop permissions and monitor availability".into()))
        })
    }
    pub fn next_rgba(&mut self) -> Result<Vec<u8>> {
        if let Some(first) = self.first.take() {
            return Ok(first);
        }
        let (bytes, source_dimensions, dimensions) = rgba(&self.sample()?)?;
        if source_dimensions != self.source_dimensions || dimensions != self.dimensions {
            return Err(error(
                "display dimensions changed; reconnect to renegotiate video",
            ));
        }
        Ok(bytes)
    }
}
impl trinix_core::Capture for WindowsCapture {
    fn next_frame(&mut self) -> Option<Vec<u8>> {
        match self.next_rgba() {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                eprintln!("desktop capture stopped: {error}");
                None
            }
        }
    }
}
impl Drop for WindowsCapture {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
type RgbaSample = (Vec<u8>, (u32, u32), (u32, u32));
fn rgba(sample: &gst::Sample) -> Result<RgbaSample> {
    let info =
        video::VideoInfo::from_caps(sample.caps().ok_or_else(|| error("missing capture caps"))?)
            .map_err(error)?;
    if info.format() != video::VideoFormat::Rgba {
        return Err(error("capture is not RGBA"));
    }
    let buffer = sample
        .buffer()
        .ok_or_else(|| error("missing capture buffer"))?;
    let frame = video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).map_err(error)?;
    let stride = usize::try_from(info.stride()[0]).map_err(|_| error("negative capture stride"))?;
    let width = info.width();
    let height = info.height();
    let bytes = pack(frame.plane_data(0).map_err(error)?, width, height, stride)?;
    Ok((bytes, (width, height), (width & !1, height & !1)))
}
/// H.264/I420 needs even dimensions. Crop only the last odd row/column.
fn pack(data: &[u8], width: u32, height: u32, stride: usize) -> Result<Vec<u8>> {
    if !(2..=8192).contains(&width) || !(2..=8192).contains(&height) {
        return Err(error("capture dimensions must be 2..=8192"));
    }
    let row_size = (width as usize & !1) * 4;
    let rows = height as usize & !1;
    if stride < width as usize * 4 {
        return Err(error("capture stride too small"));
    }
    let required = (rows - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(row_size))
        .ok_or_else(|| error("capture stride overflow"))?;
    if data.len() < required {
        return Err(error("capture plane truncated"));
    }
    let mut bytes = Vec::with_capacity(row_size * rows);
    for row in 0..rows {
        bytes.extend_from_slice(&data[row * stride..row * stride + row_size]);
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crops_odd_edges_and_strips_row_padding() {
        let data: Vec<_> = (0..48).collect();
        assert_eq!(
            pack(&data, 3, 3, 16).unwrap(),
            [&data[..8], &data[16..24]].concat()
        );
        assert!(pack(&data[..23], 3, 3, 16).is_err());
        assert!(pack(&data, 3, 3, 11).is_err());
        assert!(pack(&data, 2, 2, usize::MAX).is_err());
        assert!(pack(&data, 1, 2, 8).is_err());
        assert!(pack(&data, 8193, 2, 32772).is_err());
    }
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an interactive Windows desktop and GPU"]
    fn windows_desktop_capture_smoke() {
        let mut capture = WindowsCapture::open(30, -1).unwrap();
        let (width, height) = capture.dimensions();
        assert_eq!(
            capture.next_rgba().unwrap().len(),
            width as usize * height as usize * 4
        );
    }
    #[test]
    fn real_sample_pipeline_returns_packed_frames_and_stops() {
        let mut capture = WindowsCapture::from_description(
            "videotestsrc num-buffers=2 is-live=true ! video/x-raw,format=RGBA,width=66,height=34,framerate=30/1 ! appsink name=capture sync=false max-buffers=1"
        ).unwrap();
        assert_eq!(capture.dimensions(), (66, 34));
        assert_eq!(capture.next_rgba().unwrap().len(), 66 * 34 * 4);
        assert_eq!(capture.next_rgba().unwrap().len(), 66 * 34 * 4);
        assert!(capture.next_rgba().is_err());
    }
}
