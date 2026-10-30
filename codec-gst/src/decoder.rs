use crate::{error, Pipeline, Result};
use gstreamer as gst;
use gstreamer_video as video;

// A single compressed access unit must fit within 16 MiB, even at maximum resolution.
const MAX_ENCODED_SIZE: usize = 16 * 1024 * 1024;

fn validate_input(encoded: &[u8]) -> Result<()> {
    if encoded.is_empty() || encoded.len() > MAX_ENCODED_SIZE {
        return Err(error("H.264 access unit must contain 1..=16777216 bytes"));
    }
    Ok(())
}

fn decoded_layout(w: usize, h: usize) -> Result<(usize, usize, usize)> {
    if w == 0 || h == 0 || w > 8192 || h > 8192 || w % 2 != 0 || h % 2 != 0 {
        return Err(error("decoded dimensions must be nonzero, even and <=8192"));
    }
    let cw = w
        .checked_add(1)
        .ok_or_else(|| error("chroma width overflow"))?
        / 2;
    let ch = h
        .checked_add(1)
        .ok_or_else(|| error("chroma height overflow"))?
        / 2;
    let size = w
        .checked_mul(h)
        .and_then(|y| {
            cw.checked_mul(ch)
                .and_then(|c| c.checked_mul(2))
                .and_then(|c| y.checked_add(c))
        })
        .ok_or_else(|| error("decoded size overflow"))?;
    Ok((cw, ch, size))
}

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
    /// Decode one complete Annex B access unit (at most 16 MiB) from the matching encoder.
    /// May block for up to 3 seconds waiting for output.
    pub fn decode_frame(&mut self, encoded: &[u8], timestamp_us: u64) -> Result<DecodedFrame> {
        validate_input(encoded)?;
        let sample = self.pipe.push(encoded.to_vec(), timestamp_us, None)?;
        let info =
            video::VideoInfo::from_caps(sample.caps().ok_or_else(|| error("missing video caps"))?)
                .map_err(error)?;
        let w = info.width() as usize;
        let h = info.height() as usize;
        let (cw, ch, size) = decoded_layout(w, h)?;
        let frame = video::VideoFrameRef::from_buffer_ref_readable(
            sample
                .buffer()
                .ok_or_else(|| error("missing decoded buffer"))?,
            &info,
        )
        .map_err(error)?;
        let mut data = Vec::with_capacity(size);
        for plane in 0..3 {
            let (width, height) = if plane == 0 { (w, h) } else { (cw, ch) };
            let bytes = frame.plane_data(plane).map_err(error)?;
            let stride = usize::try_from(info.stride()[plane as usize])
                .map_err(|_| error("negative decoded stride"))?;
            if stride < width {
                return Err(error("decoded stride is smaller than plane width"));
            }
            for row in 0..height {
                let start = row
                    .checked_mul(stride)
                    .ok_or_else(|| error("row offset overflow"))?;
                let end = start
                    .checked_add(width)
                    .ok_or_else(|| error("row end overflow"))?;
                data.extend_from_slice(
                    bytes
                        .get(start..end)
                        .ok_or_else(|| error("truncated decoded plane"))?,
                );
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
impl trinix_core::traits::Decoder for Decoder {
    fn decode(&mut self, encoded: &[u8]) -> std::result::Result<Vec<u8>, trinix_core::CodecError> {
        let pts = self.next_pts;
        Ok(self.decode_frame(encoded, pts)?.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_oversized_access_units() {
        assert!(validate_input(&[]).is_err());
        let data = vec![0; MAX_ENCODED_SIZE + 1];
        assert!(validate_input(&data).is_err());
        assert!(validate_input(&data[..MAX_ENCODED_SIZE]).is_ok());
    }

    #[test]
    fn rejects_invalid_decoded_dimensions() {
        for (w, h) in [
            (0, 2),
            (2, 0),
            (1, 2),
            (2, 3),
            (8194, 2),
            (2, 8194),
            (usize::MAX, usize::MAX),
        ] {
            assert!(decoded_layout(w, h).is_err(), "accepted {w}x{h}");
        }
        assert_eq!(
            decoded_layout(8192, 8192).unwrap(),
            (4096, 4096, 100_663_296)
        );
        assert_eq!(decoded_layout(66, 34).unwrap(), (33, 17, 3366));
    }
}
