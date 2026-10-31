#![cfg(any(target_os = "linux", target_os = "windows"))]
use codec_gst::{
    decoder::Decoder,
    encoder::{Encoder, EncoderConfig, PixelFormat},
};
use std::time::Instant;

fn available() -> bool {
    if let Err(e) = gstreamer::init() {
        assert_ne!(
            std::env::var("MM_REQUIRE_CODECS").as_deref(),
            Ok("1"),
            "Required GStreamer initialization failed: {e}"
        );
        eprintln!("SKIP GStreamer codec tests: {e}");
        return false;
    }
    let missing: Vec<_> = ["appsrc", "appsink", "videoconvert", "x264enc", "avdec_h264"]
        .into_iter()
        .filter(|name| gstreamer::ElementFactory::find(name).is_none())
        .collect();
    if !missing.is_empty() {
        assert_ne!(
            std::env::var("MM_REQUIRE_CODECS").as_deref(),
            Ok("1"),
            "Required codec plugins missing: {missing:?}"
        );
        eprintln!("SKIP GStreamer codec tests: missing plugins {missing:?}");
        return false;
    }
    true
}
fn config(w: u32, h: u32) -> EncoderConfig {
    EncoderConfig {
        width: w,
        height: h,
        ..Default::default()
    }
}
fn synthetic(w: usize, h: usize, n: usize) -> Vec<u8> {
    let mut data = vec![128; w * h * 3 / 2];
    for y in 0..h {
        for x in 0..w {
            data[y * w + x] = (32 + ((x / 8 + y / 8 + n) % 160)) as u8;
        }
    }
    data
}
fn idr(data: &[u8]) -> bool {
    data.windows(4)
        .any(|v| v[..3] == [0, 0, 1] && v[3] & 31 == 5)
}
#[test]
fn encode_decode_roundtrip_psnr() {
    if !available() {
        return;
    }
    // Width deliberately requires GStreamer row padding.
    let (w, h) = (318, 178);
    let mut enc = Encoder::new(config(w, h)).unwrap();
    let mut dec = Decoder::new().unwrap();
    for n in 0..8 {
        let raw = synthetic(w as usize, h as usize, n);
        let packet = enc.encode_frame(&raw, n as u64 * 33_333).unwrap();
        let frame = dec.decode_frame(&packet.data, packet.timestamp_us).unwrap();
        assert_eq!(
            (frame.width, frame.height, frame.timestamp_us),
            (w, h, packet.timestamp_us)
        );
        assert_eq!(frame.data.len(), raw.len());
        let mse = raw
            .iter()
            .zip(&frame.data)
            .map(|(&a, &b)| (a as f64 - b as f64).powi(2))
            .sum::<f64>()
            / raw.len() as f64;
        let psnr = 10.0 * (255.0f64.powi(2) / mse).log10();
        eprintln!("frame {n}: PSNR {psnr:.2} dB");
        assert!(psnr > 30.0);
    }
}
#[test]
fn set_bitrate_changes_output_size() {
    if !available() {
        return;
    }
    let mut cfg = config(640, 360);
    cfg.bitrate_kbps = 300;
    cfg.key_int_max = 300;
    let mut enc = Encoder::new(cfg).unwrap();
    let mut state = 123u32;
    let mut sizes = [0usize; 2];
    for n in 0..100 {
        if n == 50 {
            enc.set_bitrate_kbps(6000).unwrap();
        }
        let mut raw = vec![128; 640 * 360 * 3 / 2];
        for v in &mut raw[..640 * 360] {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *v = state as u8;
        }
        let packet = enc.encode_frame(&raw, n * 33_333).unwrap();
        if n % 50 >= 20 {
            sizes[(n / 50) as usize] += packet.data.len();
        }
    }
    eprintln!("bitrate sizes: {sizes:?}");
    assert!(sizes[1] > sizes[0] * 2);
}
#[test]
fn force_keyframe_emits_idr() {
    if !available() {
        return;
    }
    let mut cfg = config(320, 180);
    cfg.key_int_max = 300;
    let mut enc = Encoder::new(cfg).unwrap();
    for n in 0..5 {
        if n == 3 {
            enc.force_keyframe().unwrap();
        }
        let packet = enc
            .encode_frame(&synthetic(320, 180, n), n as u64 * 33_333)
            .unwrap();
        assert_eq!(idr(&packet.data), n == 0 || n == 3);
        assert_eq!(packet.is_keyframe, idr(&packet.data));
    }
}
#[test]
fn encode_latency_1080p() {
    if !available() {
        return;
    }
    let mut enc = Encoder::new(EncoderConfig::default()).unwrap();
    let mut times = Vec::new();
    for n in 0..35 {
        let raw = synthetic(1920, 1080, n);
        let start = Instant::now();
        enc.encode_frame(&raw, n as u64 * 33_333).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("1080p encode frame {n}: {ms:.3} ms");
        if n >= 5 {
            times.push(ms);
        }
    }
    times.sort_by(f64::total_cmp);
    let p50 = times[times.len() / 2];
    eprintln!("1080p encode p50 {p50:.3} ms; target 10 ms");
    assert!(
        p50 < 40.0,
        "encode p50 {p50:.3} ms exceeds 40 ms sanity bound"
    );
}
#[test]
fn rgba_and_trait_adapters() {
    if !available() {
        return;
    }
    use trinix_core::traits::{Decoder as _, Encoder as _};
    let mut cfg = config(66, 34);
    cfg.format = PixelFormat::Rgba;
    let mut enc = Encoder::new(cfg).unwrap();
    assert!(enc.encode_frame(&[], 0).is_err());
    assert!(enc.set_bitrate_kbps(0).is_err());
    let raw = [50, 100, 150, 255].repeat(66 * 34);
    assert!(enc.encode(&[]).is_err());
    assert!(trinix_core::Encoder::set_bitrate_kbps(&mut enc, 0).is_err());
    let packet = enc.encode(&raw).unwrap();
    let mut dec = Decoder::new().unwrap();
    assert!(dec.decode(&[]).is_err());
    assert!(dec.decode(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
    assert_eq!(dec.decode(&packet).unwrap().len(), 66 * 34 * 3 / 2);
}

#[test]
fn hardware_preference_falls_back_and_validates_config() {
    if !available() {
        return;
    }
    let mut cfg = config(160, 90);
    cfg.prefer_hardware = true;
    let mut enc = Encoder::new(cfg).unwrap();
    let packet = enc.encode_frame(&synthetic(160, 90, 0), 0).unwrap();
    eprintln!("hardware preference selected {}", enc.backend());
    assert!(idr(&packet.data));
    assert!(Encoder::new(config(0, 90)).is_err());
    assert!(Encoder::new(config(161, 90)).is_err());
}
