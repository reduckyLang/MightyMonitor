//! Managed shutdown does not require capture or a display.
#![cfg(any(target_os = "linux", target_os = "windows"))]
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn closed_stdin_stops_managed_share() {
    let directory = std::env::temp_dir().join(format!("mm-managed-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_mm-host"));
    command
        .args(["--secure", "--managed", "--bind", "127.0.0.1:0"])
        .env("XDG_CONFIG_HOME", &directory)
        .env("APPDATA", &directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "linux")]
    command.args(["--capture", "x11"]);
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = std::fs::remove_dir_all(directory);
    assert!(result.is_some_and(|status| status.success()));
}
