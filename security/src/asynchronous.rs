//! Tokio TLS channels. A single deadline covers preface, TLS and authorization.
use super::*;
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};
pub async fn connect(
    mut stream: TcpStream,
    invitation: &Invitation,
) -> io::Result<tokio_rustls::client::TlsStream<TcpStream>> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        stream.set_nodelay(true)?;
        let mut magic = [0; 8];
        stream.read_exact(&mut magic).await?;
        if &magic != MAGIC {
            return Err(invalid("This endpoint is not a secure Trinix host"));
        }
        let mut length = [0; 2];
        stream.read_exact(&mut length).await?;
        let length = usize::from(u16::from_le_bytes(length));
        if length == 0 || length > CERT_LIMIT {
            return Err(invalid("Invalid sharing certificate length"));
        }
        let mut certificate = vec![0; length];
        stream.read_exact(&mut certificate).await?;
        let config = invitation.client_config(certificate)?;
        let mut tls = TlsConnector::from(config)
            .connect(
                ServerName::try_from(SERVER_NAME).map_err(tls_error)?,
                stream,
            )
            .await?;
        if tls.get_ref().1.alpn_protocol() != Some(ALPN) {
            return Err(invalid("Secure protocol was not negotiated"));
        }
        tls.write_all(invitation.authentication().as_slice())
            .await?;
        tls.flush().await?;
        let mut accepted = [0];
        tls.read_exact(&mut accepted).await?;
        if accepted != [1] {
            return Err(denied());
        }
        Ok(tls)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Secure connection timed out"))?
}
pub async fn accept(
    mut stream: TcpStream,
    identity: &ShareIdentity,
) -> io::Result<tokio_rustls::server::TlsStream<TcpStream>> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        stream.set_nodelay(true)?;
        if identity.certificate.len() > CERT_LIMIT {
            return Err(invalid("Sharing certificate is too large"));
        }
        stream.write_all(MAGIC).await?;
        stream
            .write_all(&(identity.certificate.len() as u16).to_le_bytes())
            .await?;
        stream.write_all(identity.certificate.as_ref()).await?;
        stream.flush().await?;
        let mut tls = TlsAcceptor::from(identity.config.clone())
            .accept(stream)
            .await?;
        if tls.get_ref().1.alpn_protocol() != Some(ALPN) {
            return Err(invalid("Secure protocol was not negotiated"));
        }
        let mut authentication = Zeroizing::new([0; 40]);
        tls.read_exact(authentication.as_mut()).await?;
        if !identity.authenticate(&authentication) {
            let _ = tls.write_all(&[0]).await;
            let _ = tls.flush().await;
            return Err(denied());
        }
        tls.write_all(&[1]).await?;
        tls.flush().await?;
        Ok(tls)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Secure connection timed out"))?
}

/// Connect with the existing bounded core framing. Authentication failures are
/// terminal to reconnect policy; network outages remain recoverable.
pub async fn connect_transport(
    endpoint: &Endpoint,
) -> Result<Box<dyn trinix_core::Transport>, trinix_core::TransportError> {
    use trinix_core::{FramedTransport, TcpTransport, TransportError};
    match endpoint {
        Endpoint::Legacy(address) => Ok(Box::new(TcpTransport::connect(*address).await?)),
        Endpoint::Secure(invitation) => {
            let stream = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                TcpStream::connect(invitation.address()),
            )
            .await
            .map_err(|_| TransportError::Timeout("connect"))??;
            let tls = connect(stream, invitation)
                .await
                .map_err(|error| match error.kind() {
                    io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidData => {
                        TransportError::Authentication(error.to_string())
                    }
                    _ => TransportError::Io(error),
                })?;
            Ok(Box::new(FramedTransport::from_stream(tls)))
        }
    }
}
