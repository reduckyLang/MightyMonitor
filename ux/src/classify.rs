//! Conversion of standard and transport errors to user-facing messages.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

use trinix_core::{FrameError, TransportError};

use crate::catalog::{Platform, UserError, UserMessage};

/// Classifies an error chain and keeps the original error text in `technical`.
pub fn classify(err: &(dyn Error + 'static)) -> UserMessage {
    let technical = err.to_string();
    let mut current = Some(err);

    while let Some(cause) = current {
        if let Some(transport) = cause.downcast_ref::<TransportError>() {
            match transport {
                TransportError::Authentication(_) => {
                    return with_technical(UserError::ConnectionExpired, technical);
                }
                TransportError::Timeout(_) => {
                    return with_technical(UserError::HostUnreachable, technical);
                }
                TransportError::Closed => {
                    return with_technical(UserError::HostStopped, technical);
                }
                TransportError::Frame(FrameError::UnsupportedVersion(_)) => {
                    return with_technical(UserError::VersionMismatch, technical);
                }
                TransportError::Frame(_) => {
                    return with_technical(UserError::DecodeFailed, technical);
                }
                TransportError::Io(_) => {}
            }
        }

        if let Some(io_error) = cause.downcast_ref::<IoError>() {
            if let Some(error) = classify_io_error(io_error) {
                return with_technical(error, technical);
            }
        }

        if let Some(error) = classify_message(&cause.to_string()) {
            return with_technical(error, technical);
        }

        current = cause.source();
    }

    with_technical(UserError::Unknown, technical)
}

fn with_technical(error: UserError, technical: String) -> UserMessage {
    let mut message = error.message(Platform::Any);
    message.technical = Some(technical);
    message
}

fn classify_io_error(error: &IoError) -> Option<UserError> {
    match error.kind() {
        ErrorKind::ConnectionRefused | ErrorKind::TimedOut => Some(UserError::HostUnreachable),
        ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::BrokenPipe
        | ErrorKind::UnexpectedEof => Some(UserError::ConnectionLost),
        ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::AddrNotAvailable => Some(UserError::SameWifi),
        ErrorKind::NotConnected => Some(UserError::NoNetwork),
        ErrorKind::PermissionDenied => Some(UserError::FirewallBlocked),
        ErrorKind::InvalidData => {
            let text = error.to_string().to_ascii_lowercase();
            (text.contains("tls") || text.contains("certificate") || text.contains("handshake"))
                .then_some(UserError::ConnectionExpired)
        }
        _ => None,
    }
}

fn classify_message(message: &str) -> Option<UserError> {
    let normalized = message.to_ascii_lowercase();

    if has_authentication_or_pin_mismatch(&normalized) {
        return Some(UserError::ConnectionExpired);
    }
    if normalized.starts_with("secure connection failed") {
        return Some(UserError::ConnectFailed);
    }
    if normalized.starts_with("deadline has elapsed") {
        return Some(UserError::HostUnreachable);
    }
    if normalized.starts_with("connection refused") {
        return Some(UserError::HostUnreachable);
    }
    if normalized.starts_with("no route to host")
        || normalized.starts_with("network is unreachable")
    {
        return Some(UserError::SameWifi);
    }
    if normalized.starts_with("unsupported protocol version")
        || (normalized.contains("version") && normalized.contains("mismatch"))
    {
        return Some(UserError::VersionMismatch);
    }

    None
}

fn has_authentication_or_pin_mismatch(message: &str) -> bool {
    message.contains("auth mismatch")
        || message.contains("authentication mismatch")
        || message.contains("pin mismatch")
        || message.contains("certificate mismatch")
        || message.contains("identity mismatch")
        || message.contains("fingerprint mismatch")
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fmt;
    use std::io::{Error as IoError, ErrorKind};

    use trinix_core::{FrameError, TransportError};

    use crate::catalog::UserError;

    use super::classify;

    #[derive(Debug)]
    struct MessageError(&'static str);

    impl fmt::Display for MessageError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl Error for MessageError {}

    fn assert_classified(error: &(dyn Error + 'static), expected: UserError) {
        let message = classify(error);
        assert_eq!(message.title, expected.title());
        assert_eq!(message.next_step, expected.next_step());
        assert_eq!(
            message.technical.as_deref(),
            Some(error.to_string().as_str())
        );
    }

    #[test]
    fn classifies_standard_io_errors() {
        assert_classified(
            &IoError::from(ErrorKind::ConnectionRefused),
            UserError::HostUnreachable,
        );
        assert_classified(
            &IoError::from(ErrorKind::TimedOut),
            UserError::HostUnreachable,
        );
        assert_classified(
            &IoError::from(ErrorKind::HostUnreachable),
            UserError::SameWifi,
        );
        assert_classified(
            &IoError::from(ErrorKind::NetworkUnreachable),
            UserError::SameWifi,
        );
        assert_classified(
            &IoError::from(ErrorKind::AddrNotAvailable),
            UserError::SameWifi,
        );
        assert_classified(
            &IoError::from(ErrorKind::NotConnected),
            UserError::NoNetwork,
        );
        assert_classified(
            &IoError::from(ErrorKind::PermissionDenied),
            UserError::FirewallBlocked,
        );
        assert_classified(
            &IoError::new(ErrorKind::InvalidData, "TLS handshake failed"),
            UserError::ConnectionExpired,
        );
        assert_classified(
            &TransportError::Io(IoError::from(ErrorKind::BrokenPipe)),
            UserError::ConnectionLost,
        );
        assert_classified(
            &IoError::from(ErrorKind::UnexpectedEof),
            UserError::ConnectionLost,
        );
        assert_classified(
            &IoError::from(ErrorKind::ConnectionReset),
            UserError::ConnectionLost,
        );
        assert_classified(
            &IoError::from(ErrorKind::ConnectionAborted),
            UserError::ConnectionLost,
        );
    }

    #[test]
    fn classifies_known_error_messages() {
        assert_classified(
            &MessageError("deadline has elapsed"),
            UserError::HostUnreachable,
        );
        assert_classified(
            &MessageError("Secure connection failed: refused"),
            UserError::ConnectFailed,
        );
        assert_classified(
            &MessageError("Secure connection failed: pin mismatch"),
            UserError::ConnectionExpired,
        );
        assert_classified(
            &MessageError("Connection refused by peer"),
            UserError::HostUnreachable,
        );
        assert_classified(&MessageError("No route to host"), UserError::SameWifi);
        assert_classified(&MessageError("Network is unreachable"), UserError::SameWifi);
        assert_classified(
            &MessageError("Unsupported protocol version 1"),
            UserError::VersionMismatch,
        );
        assert_classified(
            &MessageError("version mismatch"),
            UserError::VersionMismatch,
        );
    }

    #[test]
    fn classifies_transport_errors() {
        assert_classified(
            &TransportError::Authentication("bad identity".into()),
            UserError::ConnectionExpired,
        );
        assert_classified(&TransportError::Timeout("read"), UserError::HostUnreachable);
        assert_classified(&TransportError::Closed, UserError::HostStopped);
        assert_classified(
            &TransportError::Frame(FrameError::Empty),
            UserError::DecodeFailed,
        );
        assert_classified(
            &TransportError::Frame(FrameError::UnsupportedVersion(2)),
            UserError::VersionMismatch,
        );
    }

    #[test]
    fn permission_denied_is_not_reported_as_an_expired_connection() {
        let message = classify(&IoError::from(ErrorKind::PermissionDenied));
        assert_ne!(message.title, UserError::ConnectionExpired.title());
    }

    #[test]
    fn unknown_errors_preserve_technical_text() {
        let error = MessageError("some diagnostic detail");
        let message = classify(&error);
        assert_eq!(message.title, UserError::Unknown.title());
        assert_eq!(message.technical.as_deref(), Some("some diagnostic detail"));
    }

    #[test]
    fn technical_text_never_in_title() {
        let errors: [&(dyn Error + 'static); 3] = [
            &IoError::from(ErrorKind::ConnectionRefused),
            &TransportError::Timeout("read"),
            &MessageError("private diagnostic text"),
        ];

        for error in errors {
            let message = classify(error);
            let technical = message.technical.as_deref().expect("technical text");
            assert!(!message.title.contains(technical));
            assert!(!message.next_step.contains(technical));
        }
    }

    #[test]
    fn walks_wrapped_error_sources() {
        #[derive(Debug)]
        struct Wrapped(IoError);

        impl fmt::Display for Wrapped {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("outer error")
            }
        }

        impl Error for Wrapped {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                Some(&self.0)
            }
        }

        assert_classified(
            &Wrapped(IoError::from(ErrorKind::ConnectionRefused)),
            UserError::HostUnreachable,
        );
    }
}
