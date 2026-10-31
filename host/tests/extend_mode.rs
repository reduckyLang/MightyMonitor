//! Unsupported extension must fail before capture or network sharing begins.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
fn unsupported_extension_does_not_silently_mirror() {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_trinix-host"));
    command.args(["--secure", "--extend", "--bind", "127.0.0.1:0"]);
    #[cfg(target_os = "linux")]
    command.args(["--capture", "x11"]);
    let output = command.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "must not publish an invitation");
    let error = String::from_utf8_lossy(&output.stderr);
    #[cfg(target_os = "linux")]
    assert!(error.contains("Extended displays currently require a Wayland desktop"));
    #[cfg(target_os = "windows")]
    assert!(error.contains("Windows extended displays require a virtual display driver"));
}
