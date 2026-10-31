//! Background discovery and explicit pairing decisions, independent of drawing.
use mightymonitor_security::{
    pairing::{DeviceIdentity, PairingOffer},
    store::DeviceStore,
};
use std::{io, net::SocketAddr, path::PathBuf, sync::mpsc, time::Duration};
/// The device endpoint routes pairing and paired viewing before pinned TLS.
pub async fn dial(address: SocketAddr, pair: bool) -> io::Result<tokio::net::TcpStream> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        let mut magic = [0; 8];
        stream.read_exact(&mut magic).await?;
        if &magic != b"MMROUTE1" {
            return Err(io::Error::other("Invalid device response"));
        }
        stream.write_u8(u8::from(pair)).await?;
        Ok(stream)
    })
    .await
    .map_err(|_| io::Error::other("Device did not answer"))?
}
pub fn browse() -> io::Result<mpsc::Receiver<Vec<mightymonitor_discovery::Device>>> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("mm-nearby".into())
        .spawn(move || {
            let result = mightymonitor_discovery::Mdns::new().and_then(|mut backend| {
                mightymonitor_discovery::browse(&mut backend, Duration::from_secs(3))
            });
            let _ = tx.try_send(result.unwrap_or_default());
        })?;
    Ok(rx)
}
pub enum PairEvent {
    Code(String, tokio::sync::oneshot::Sender<bool>),
    Done(Result<[u8; 32], ()>),
}
pub async fn pair(
    address: SocketAddr,
    directory: PathBuf,
    offer: Option<PairingOffer>,
    events: mpsc::SyncSender<PairEvent>,
) {
    let result = async {
        let identity = DeviceIdentity::open(&directory)?;
        let stream = dial(address, true).await?;
        let confirm_events = events.clone();
        let confirm = move |code| async move {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if confirm_events.try_send(PairEvent::Code(code, tx)).is_err() {
                return false;
            }
            rx.await.unwrap_or(false)
        };
        let peer = match offer {
            Some(offer) => {
                mightymonitor_security::pairing::asynchronous::pair_client_offer(
                    stream, &identity, &offer, confirm,
                )
                .await?
            }
            None => {
                mightymonitor_security::pairing::asynchronous::pair_client(
                    stream, &identity, confirm,
                )
                .await?
            }
        };
        let fingerprint = peer.fingerprint;
        DeviceStore::open(&directory)?.insert(peer)?;
        Ok::<_, io::Error>(fingerprint)
    }
    .await;
    let _ = events.try_send(PairEvent::Done(result.map_err(|_| ())));
}
