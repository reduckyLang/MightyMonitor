//! Synchronous, bounded, low-latency H.264 codecs. See docs/CODEC.md.
#![forbid(unsafe_code)]
#![cfg(target_os = "linux")]

pub mod decoder;
pub mod encoder;

use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSink, AppSrc};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CodecError(pub String);
pub type Result<T> = std::result::Result<T, CodecError>;

pub(crate) fn error(e: impl std::fmt::Display) -> CodecError {
    CodecError(e.to_string())
}

pub(crate) struct Pipeline {
    pipeline: gst::Pipeline,
    source: AppSrc,
    sink: AppSink,
    failed: bool,
}
impl Pipeline {
    fn new(description: &str, caps: &gst::Caps) -> Result<Self> {
        gst::init().map_err(error)?;
        let pipeline = gst::parse::launch(description)
            .map_err(error)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| error("expected pipeline"))?;
        let source = pipeline
            .by_name("input")
            .ok_or_else(|| error("missing pipeline input"))?
            .downcast::<AppSrc>()
            .map_err(|_| error("pipeline input is not appsrc"))?;
        let sink = pipeline
            .by_name("output")
            .ok_or_else(|| error("missing pipeline output"))?
            .downcast::<AppSink>()
            .map_err(|_| error("pipeline output is not appsink"))?;
        source.set_caps(Some(caps));
        let pipe = Self {
            pipeline,
            source,
            sink,
            failed: false,
        };
        pipe.pipeline
            .set_state(gst::State::Playing)
            .map_err(error)?;
        Ok(pipe)
    }
    fn push(&mut self, data: Vec<u8>, pts: u64, duration: Option<u64>) -> Result<gst::Sample> {
        if self.failed {
            return Err(error(
                "codec failed; recreate it before submitting another frame",
            ));
        }
        if pts > u64::MAX / 1000 {
            return Err(error("timestamp overflows GStreamer clock"));
        }
        // Poison until output arrives: a late sample must never be assigned to a later input.
        self.failed = true;
        let mut buffer = gst::Buffer::from_mut_slice(data);
        let writable = buffer
            .get_mut()
            .ok_or_else(|| error("input buffer is not writable"))?;
        writable.set_pts(gst::ClockTime::from_useconds(pts));
        writable.set_duration(duration.map(gst::ClockTime::from_useconds));
        self.source.push_buffer(buffer).map_err(error)?;
        let sample = self
            .sink
            .try_pull_sample(gst::ClockTime::from_seconds(3))
            .ok_or_else(|| {
                let detail = self
                    .pipeline
                    .bus()
                    .and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]))
                    .map(|m| format!("{m:?}"));
                error(detail.unwrap_or_else(|| "codec timed out waiting for output".into()))
            })?;
        self.failed = false;
        Ok(sample)
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
