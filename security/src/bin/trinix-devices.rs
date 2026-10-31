//! Interactive security-crate helper; contains no discovery or viewer code.
#![forbid(unsafe_code)]
#[cfg(feature = "async")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use trinix_security::{
        device_cli,
        pairing::{asynchronous, DeviceIdentity},
        store::{config_dir, DeviceStore},
    };
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--pair") => {
            let address: std::net::SocketAddr = args.next().ok_or("Missing address")?.parse()?;
            if args.next().is_some() {
                return Err("Unexpected argument".into());
            }
            let directory = config_dir()?;
            let identity = DeviceIdentity::open(&directory)?;
            let stream = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio::net::TcpStream::connect(address),
            )
            .await??;
            let peer = asynchronous::pair_client(stream, &identity, device_cli::confirm).await?;
            DeviceStore::open(&directory)?.insert(peer)?;
        }
        Some(flag @ ("--list-paired" | "--unpair" | "--forget-all")) => {
            device_cli::maintenance(flag, args.next().as_deref())?
        }
        _ => return Err(
            "Use trinix-devices --pair ADDRESS:PORT | --list-paired | --unpair ID | --forget-all"
                .into(),
        ),
    }
    Ok(())
}
#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("trinix-devices requires the async feature");
}
