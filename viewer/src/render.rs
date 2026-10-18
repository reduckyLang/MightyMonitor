//! Packed I420 to RGB conversion for the texture upload.
use crate::net::Picture;
pub fn rgb(p: &Picture) -> Option<Vec<u8>> {
    let w = p.width as usize;
    let h = p.height as usize;
    if w == 0
        || h == 0
        || w % 2 != 0
        || h % 2 != 0
        || p.i420.len() != w.checked_mul(h)?.checked_mul(3)? / 2
    {
        return None;
    }
    let y_size = w * h;
    let mut rgb = Vec::with_capacity(y_size * 3);
    for y in 0..h {
        for x in 0..w {
            let c = p.i420[y * w + x] as f32 - 16.0;
            let uv = (y / 2) * (w / 2) + x / 2;
            let d = p.i420[y_size + uv] as f32 - 128.0;
            let e = p.i420[y_size + y_size / 4 + uv] as f32 - 128.0;
            for v in [
                1.164 * c + 1.596 * e,
                1.164 * c - 0.392 * d - 0.813 * e,
                1.164 * c + 2.017 * d,
            ] {
                rgb.push(v.clamp(0.0, 255.0) as u8);
            }
        }
    }
    Some(rgb)
}
