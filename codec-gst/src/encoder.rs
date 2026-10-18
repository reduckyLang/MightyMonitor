use crate::{error, Pipeline, Result};
use gstreamer::{self as gst, prelude::*};
use gstreamer_video as video;

#[derive(Clone, Copy, Debug)]
pub enum PixelFormat {
    I420,
    Rgba,
}
impl PixelFormat {
    fn gst(self) -> video::VideoFormat {
        match self {
            Self::I420 => video::VideoFormat::I420,
            Self::Rgba => video::VideoFormat::Rgba,
        }
    }
}
#[derive(Clone, Debug)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub key_int_max: u32,
    pub prefer_hardware: bool,
}
impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            format: PixelFormat::I420,
            fps: 30,
            bitrate_kbps: 8000,
            key_int_max: 30,
            prefer_hardware: false,
        }
    }
}
#[derive(Debug)]
pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub timestamp_us: u64,
    pub is_keyframe: bool,
}

pub struct Encoder {
    pipe: Pipeline,
    config: EncoderConfig,
    info: video::VideoInfo,
    backend: String,
    next_pts: u64,
}
impl Encoder {
    pub fn new(config: EncoderConfig) -> Result<Self> {
        gst::init().map_err(error)?;
        if config.width == 0
            || config.height == 0
            || config.width > 8192
            || config.height > 8192
            || config.width % 2 != 0
            || config.height % 2 != 0
            || config.fps == 0
            || config.fps > 240
            || config.bitrate_kbps == 0
            || config.bitrate_kbps > 1_000_000
            || config.key_int_max == 0
            || config.key_int_max > i32::MAX as u32
        {
            return Err(error("invalid dimensions, fps, bitrate or GOP"));
        }
        let info = video::VideoInfo::builder(config.format.gst(), config.width, config.height)
            .fps(gst::Fraction::new(config.fps as i32, 1))
            .build()
            .map_err(error)?;
        let mut candidates = Vec::new();
        if config.prefer_hardware {
            for name in ["nvh264enc", "vaapih264enc"] {
                if gst::ElementFactory::find(name).is_some() {
                    candidates.push(name);
                }
            }
        }
        candidates.push("x264enc");
        let mut last_error = error("no encoder available");
        for backend in candidates {
            let settings = match backend {
                "nvh264enc" => format!("zerolatency=true bframes=0 gop-size={} bitrate={}", config.key_int_max, config.bitrate_kbps),
                "vaapih264enc" => format!("max-bframes=0 keyframe-period={} bitrate={}", config.key_int_max, config.bitrate_kbps),
                _ => format!("tune=zerolatency speed-preset=ultrafast bframes=0 key-int-max={} bitrate={} byte-stream=true threads=2", config.key_int_max, config.bitrate_kbps),
            };
            let description = format!("appsrc name=input format=time is-live=true max-buffers=1 ! videoconvert ! video/x-raw,format=I420 ! {backend} name=encoder {settings} ! video/x-h264,stream-format=byte-stream,alignment=au ! appsink name=output sync=false async=false max-buffers=1");
            let caps = info.to_caps().map_err(error)?;
            // Factories can be installed without a working device/driver. Probe actual
            // output, then recreate so the caller's first frame still carries SPS/PPS.
            if backend != "x264enc" {
                let probe = Pipeline::new(&description, &caps).and_then(|mut pipe| {
                    pipe.push(vec![0; info.size()], 0, Some(1_000_000 / config.fps as u64))
                });
                if let Err(e) = probe {
                    last_error = e;
                    continue;
                }
            }
            match Pipeline::new(&description, &caps) {
                Ok(pipe) => {
                    return Ok(Self {
                        pipe,
                        config,
                        info,
                        backend: backend.into(),
                        next_pts: 0,
                    })
                }
                Err(e) => last_error = e,
            }
        }
        Err(last_error)
    }
    pub fn backend(&self) -> &str {
        &self.backend
    }
    /// May block for up to 3 seconds waiting for output.
    /// Input is tightly packed I420 (Y,U,V) or RGBA, without row padding.
    pub fn encode_frame(&mut self, raw: &[u8], timestamp_us: u64) -> Result<EncodedFrame> {
        let w = self.config.width as usize;
        let h = self.config.height as usize;
        let expected = match self.config.format {
            PixelFormat::I420 => w * h * 3 / 2,
            PixelFormat::Rgba => w * h * 4,
        };
        if raw.len() != expected {
            return Err(error("incorrect raw frame length"));
        }
        let mut padded = vec![0; self.info.size()];
        let mut offset = 0;
        for plane in 0..self.info.n_planes() as usize {
            let (width, height) = match self.config.format {
                PixelFormat::Rgba => (w * 4, h),
                PixelFormat::I420 if plane > 0 => (w / 2, h / 2),
                _ => (w, h),
            };
            for row in 0..height {
                let start = self.info.offset()[plane] + row * self.info.stride()[plane] as usize;
                padded[start..start + width].copy_from_slice(&raw[offset..offset + width]);
                offset += width;
            }
        }
        let sample = self.pipe.push(
            padded,
            timestamp_us,
            Some(1_000_000 / self.config.fps as u64),
        )?;
        let buffer = sample
            .buffer()
            .ok_or_else(|| error("missing encoded buffer"))?;
        let data = buffer.map_readable().map_err(error)?.as_slice().to_vec();
        self.next_pts = timestamp_us.saturating_add(1_000_000 / self.config.fps as u64);
        Ok(EncodedFrame {
            data,
            timestamp_us,
            is_keyframe: !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT),
        })
    }
    pub fn set_bitrate_kbps(&mut self, kbps: u32) -> Result<()> {
        if kbps == 0 || kbps > 1_000_000 {
            return Err(error("bitrate must be 1..=1000000 kbps"));
        }
        let encoder = self
            .pipe
            .pipeline
            .by_name("encoder")
            .ok_or_else(|| error("missing encoder"))?;
        let property = encoder
            .find_property("bitrate")
            .ok_or_else(|| error("encoder has no bitrate property"))?;
        let range = property
            .downcast_ref::<gst::glib::ParamSpecUInt>()
            .ok_or_else(|| error("unsupported bitrate property type"))?;
        if kbps < range.minimum() || kbps > range.maximum() {
            return Err(error("bitrate outside backend property range"));
        }
        encoder.set_property("bitrate", kbps);
        self.config.bitrate_kbps = kbps;
        Ok(())
    }
    pub fn force_keyframe(&self) -> Result<()> {
        let event = video::UpstreamForceKeyUnitEvent::builder()
            .all_headers(true)
            .build();
        if self
            .pipe
            .pipeline
            .by_name("encoder")
            .ok_or_else(|| error("missing encoder"))?
            .static_pad("src")
            .ok_or_else(|| error("missing encoder source pad"))?
            .send_event(event)
        {
            Ok(())
        } else {
            Err(error("force-key-unit rejected"))
        }
    }
}
impl mightymonitor_core::traits::Encoder for Encoder {
    fn encode(
        &mut self,
        raw: &[u8],
    ) -> std::result::Result<Vec<u8>, mightymonitor_core::CodecError> {
        let pts = self.next_pts;
        Ok(self.encode_frame(raw, pts)?.data)
    }
    fn set_bitrate_kbps(
        &mut self,
        kbps: u32,
    ) -> std::result::Result<(), mightymonitor_core::CodecError> {
        Ok(Encoder::set_bitrate_kbps(self, kbps)?)
    }
}
