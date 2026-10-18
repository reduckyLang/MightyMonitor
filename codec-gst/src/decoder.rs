use crate::{error, Pipeline, Result};
use gstreamer as gst;
use gstreamer_video as video;

#[derive(Debug)]
pub struct DecodedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: u64,
}
pub struct Decoder {
    pipe: Pipeline,
    next_pts: u64,
}
impl Decoder {
    pub fn new() -> Result<Self> {
        gst::init().map_err(error)?;
        let caps = gst::Caps::builder("video/x-h264")
            .field("stream-format", "byte-stream")
            .field("alignment", "au")
            .build();
        let pipe = Pipeline::new("appsrc name=input format=time is-live=true max-buffers=1 ! avdec_h264 max-threads=1 ! videoconvert ! video/x-raw,format=I420 ! appsink name=output sync=false async=false max-buffers=1", &caps)?;
        Ok(Self { pipe, next_pts: 0 })
    }
    /// Decode one complete Annex B access unit from the matching encoder.
    pub fn decode_frame(&mut self, encoded: &[u8], timestamp_us: u64) -> Result<DecodedFrame> {
        if encoded.is_empty() {
            return Err(error("empty H.264 access unit"));
        }
        let sample = self.pipe.push(encoded.to_vec(), timestamp_us, None)?;
        let info =
            video::VideoInfo::from_caps(sample.caps().ok_or_else(|| error("missing video caps"))?)
                .map_err(error)?;
        let frame = video::VideoFrameRef::from_buffer_ref_readable(
            sample
                .buffer()
                .ok_or_else(|| error("missing decoded buffer"))?,
            &info,
        )
        .map_err(error)?;
        let w = info.width() as usize;
        let h = info.height() as usize;
        let mut data = Vec::with_capacity(w * h * 3 / 2);
        for plane in 0..3 {
            let (width, height) = if plane == 0 { (w, h) } else { (w / 2, h / 2) };
            let bytes = frame.plane_data(plane).map_err(error)?;
            for row in 0..height {
                let start = row * info.stride()[plane as usize] as usize;
                data.extend_from_slice(&bytes[start..start + width]);
            }
        }
        self.next_pts = timestamp_us.saturating_add(33_333);
        Ok(DecodedFrame {
            data,
            width: info.width(),
            height: info.height(),
            timestamp_us,
        })
    }
}
impl mightymonitor_core::traits::Decoder for Decoder {
    fn decode(&mut self, encoded: &[u8]) -> Vec<u8> {
        let pts = self.next_pts;
        self.decode_frame(encoded, pts)
            .expect("GStreamer decode failed; use decode_frame for recoverable errors")
            .data
    }
}
