#![cfg(all(target_os = "linux", feature = "pipewire"))]
#![forbid(unsafe_code)]

use host_linux::PipeWireCapture;
use trinix_core::Capture;

#[test]
#[ignore = "requires a Wayland session, ScreenCast portal, PipeWire and interactive consent"]
fn wayland_capture_smoke() {
    assert!(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        "run inside a Wayland desktop session"
    );
    let mut capture =
        PipeWireCapture::open().expect("authorize screen capture in the portal dialog");
    let first = capture
        .next_captured()
        .expect("first mapped frame within five seconds");
    assert!(first.width > 0 && first.height > 0);
    assert_eq!(
        first.data.len(),
        first.width as usize * first.height as usize * 4
    );
    assert!(first.data.chunks_exact(4).all(|pixel| pixel[3] == 255));
    let second = capture.next_captured().expect("second frame");
    assert!(second.timestamp_us >= first.timestamp_us);
    assert!(capture.next_frame().is_some());
}
