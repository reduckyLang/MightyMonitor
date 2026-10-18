#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use host_linux::{LinuxCapture, X11Capture};
use std::time::Instant;

#[test]
#[ignore = "requires an X11 display; run under Xvfb"]
fn x11_capture_smoke() {
    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("DISPLAY is unset; run with xvfb-run -a -s \"-screen 0 1280x720x24\" cargo test -p host-linux -- --ignored x11_capture_smoke");
        return;
    }
    let source = X11Capture::open(None).expect("open DISPLAY");
    let (width, height) = (u32::from(source.width()), u32::from(source.height()));
    assert!(width > 0 && height > 0);
    let mut capture = LinuxCapture::new(source);
    let start = Instant::now();
    for _ in 0..30 {
        let frame = capture.next_captured().expect("capture frame");
        assert_eq!((frame.width, frame.height), (width, height));
        assert_eq!(frame.data.len(), width as usize * height as usize * 4);
    }
    let fps = 30.0 / start.elapsed().as_secs_f64();
    eprintln!(
        "X11 {width}x{height}: {fps:.1} fps, last grab {:?}",
        capture.last_grab_duration()
    );
    assert!(fps >= 20.0, "effective capture fps {fps:.1} is below 20");
}
