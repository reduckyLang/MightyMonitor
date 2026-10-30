//! Headless cross-platform receiver for real encoder/transport/decoder acceptance.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let address = if args.first().map(String::as_str) == Some("--invitation-file") {
        let path = args.get(1).ok_or("Pass an invitation file")?;
        let invitation: trinix_security::Invitation =
            std::fs::read_to_string(path)?.trim().parse()?;
        if args.get(2).map(String::as_str) == Some("--via") {
            invitation
                .via_address(args.get(3).ok_or("Pass a forwarding address")?.parse()?)?
                .to_link()
        } else {
            invitation.to_link()
        }
    } else {
        args.first()
            .ok_or("Pass a connection link or invitation file")?
            .clone()
    };
    let session = trinix_viewer::net::ViewerSession::new(codec_gst::decoder::Decoder::new()?);
    let output = session.output.clone();
    let task = tokio::spawn(session.run(address));
    let start = tokio::time::Instant::now();
    let mut frames = 0;
    while start.elapsed() < Duration::from_secs(120) {
        if output
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .picture
            .take()
            .is_some()
        {
            frames += 1;
        }
        if frames >= 3 {
            task.abort();
            println!("OK: desktop decoded {frames} real video frames");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    task.abort();
    let error = output
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .error
        .clone();
    Err(format!("No desktop video: {error:?}").into())
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn main() {
    panic!("This receiver requires a desktop video decoder");
}
