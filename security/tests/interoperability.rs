#![cfg(feature = "async")]
use std::{
    io::Read,
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};
use trinix_core::{FramedTransport, Transport};
use trinix_security::{asynchronous, blocking, ShareIdentity};
#[tokio::test]
async fn blocking_host_and_async_framed_viewer_interoperate() {
    let identity = ShareIdentity::generate().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let invitation = identity.invitation(listener.local_addr().unwrap()).unwrap();
    let worker = thread::spawn(move || {
        let mut tls = blocking::accept(listener.accept().unwrap().0, &identity).unwrap();
        let mut frame = [0; 64];
        let n = tls.read(&mut frame).unwrap();
        assert!(n > 0);
        tls.write_bounded(&frame[..n], Duration::from_secs(2))
            .unwrap();
    });
    let tls = asynchronous::connect(
        tokio::net::TcpStream::connect(invitation.address())
            .await
            .unwrap(),
        &invitation,
    )
    .await
    .unwrap();
    let mut framed = FramedTransport::from_stream(tls);
    framed.send(7, b"encrypted screen data").await.unwrap();
    let frame = framed.recv().await.unwrap();
    assert_eq!(frame.kind, 7);
    assert_eq!(frame.payload, b"encrypted screen data");
    worker.join().unwrap();
}
#[tokio::test]
async fn async_host_and_blocking_viewer_interoperate() {
    let identity = ShareIdentity::generate().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let invitation = identity.invitation(listener.local_addr().unwrap()).unwrap();
    let client = tokio::task::spawn_blocking(move || {
        let mut tls = blocking::connect(
            TcpStream::connect(invitation.address()).unwrap(),
            &invitation,
        )
        .unwrap();
        tls.write_bounded(b"ping", Duration::from_secs(2)).unwrap();
        let mut response = [0; 4];
        tls.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"pong");
    });
    let mut tls = asynchronous::accept(listener.accept().await.unwrap().0, &identity)
        .await
        .unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut request = [0; 4];
    tls.read_exact(&mut request).await.unwrap();
    assert_eq!(&request, b"ping");
    tls.write_all(b"pong").await.unwrap();
    tls.flush().await.unwrap();
    client.await.unwrap();
}
#[tokio::test]
async fn expired_invitation_is_terminal_for_transport() {
    let identity = ShareIdentity::generate().unwrap();
    let old = ShareIdentity::generate().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let invite = old.invitation(listener.local_addr().unwrap()).unwrap();
    let host = tokio::spawn(async move {
        assert!(
            asynchronous::accept(listener.accept().await.unwrap().0, &identity)
                .await
                .is_err()
        );
    });
    assert!(matches!(
        asynchronous::connect_transport(&trinix_security::Endpoint::Secure(invite)).await,
        Err(trinix_core::TransportError::Authentication(_))
    ));
    host.await.unwrap();
}
