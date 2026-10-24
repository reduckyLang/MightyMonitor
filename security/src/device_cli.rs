//! Terminal-only pairing consent. Call from a CLI, never from an unattended GUI.
use super::*;
/// A detached stdin reader may remain blocked after timeout. CLI pairing exits
/// after one attempt and never shares stdin with managed Share commands.
#[cfg(feature = "async")]
pub async fn confirm(code: String) -> bool {
    eprintln!("Compare pairing code {code} on BOTH devices. Type yes to pair:");
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        use std::io::{BufRead, Read};
        let mut line = String::new();
        let accepted = std::io::stdin()
            .lock()
            .take(16)
            .read_line(&mut line)
            .is_ok()
            && line.trim() == "yes";
        let _ = tx.send(accepted);
    });
    rx.await.unwrap_or(false)
}
/// Store maintenance never generates or prints credentials.
pub fn maintenance(flag: &str, id: Option<&str>) -> io::Result<()> {
    let mut store = crate::store::DeviceStore::open(&crate::store::config_dir()?)?;
    match flag {
        "--list-paired" => {
            for peer in store.list() {
                println!(
                    "{}\t{}\t{}\t{}",
                    peer.device_id, peer.friendly_name, peer.paired_at, peer.last_seen
                );
            }
            Ok(())
        }
        "--forget-all" => store.forget_all(),
        "--unpair" => store.unpair(id.ok_or_else(|| invalid("Missing peer id"))?),
        _ => Err(invalid("Unknown device command")),
    }
}
