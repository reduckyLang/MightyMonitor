//! Canonical titles and next steps for user-facing errors.

/// A user-facing error category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UserError {
    /// The local device has no network connection.
    NoNetwork,
    /// Discovery did not find the other device on the local network.
    SameWifi,
    /// A known device did not respond.
    HostUnreachable,
    /// A connection invitation is invalid or expired.
    ConnectionExpired,
    /// The other device cannot accept another connection yet.
    HostBusy,
    /// The local system blocked a connection.
    FirewallBlocked,
    /// Video support is unavailable.
    VideoRuntimeMissing,
    /// Screen sharing permission was denied.
    ScreenPermissionDenied,
    /// Screen capture is unavailable in the current environment.
    ScreenCaptureBlocked,
    /// A second screen is unsupported.
    ExtendUnsupported,
    /// The received screen could not be decoded.
    DecodeFailed,
    /// The other device stopped sharing.
    HostStopped,
    /// The devices use different application versions.
    VersionMismatch,
    /// An unclassified error occurred.
    Unknown,
    /// A connection could not be established.
    ConnectFailed,
    /// An established connection was lost.
    ConnectionLost,
    /// No nearby devices were found.
    NothingNearby,
    /// The pairing codes do not match.
    PairingCodesDiffer,
    /// The other device declined pairing.
    PairingDeclined,
    /// Pairing did not finish before its deadline.
    PairingExpired,
    /// Too many pairing attempts were made.
    TooManyTries,
    /// This device is no longer paired.
    DeviceForgotten,
    /// Camera permission was denied.
    CameraDenied,
    /// Notifications are disabled.
    NotificationsOff,
    /// Accessibility permission is disabled.
    AccessibilityOff,
    /// The other device has not allowed control.
    ControlNotAllowed,
    /// A required application component is missing.
    SharingPartMissing,
    /// This device cannot share its screen.
    ScreenCaptureUnavailable,
    /// Sharing could not start.
    ShareCouldNotStart,
    /// The application could not start.
    StartupFailed,
    /// Sharing stopped on this device.
    HostStoppedLocal,
    /// This device cannot show a shared screen.
    CannotShowSharedScreen,
}

/// Every supported user-facing error category.
pub const ALL: &[UserError] = &[
    UserError::NoNetwork,
    UserError::SameWifi,
    UserError::HostUnreachable,
    UserError::ConnectionExpired,
    UserError::HostBusy,
    UserError::FirewallBlocked,
    UserError::VideoRuntimeMissing,
    UserError::ScreenPermissionDenied,
    UserError::ScreenCaptureBlocked,
    UserError::ExtendUnsupported,
    UserError::DecodeFailed,
    UserError::HostStopped,
    UserError::VersionMismatch,
    UserError::Unknown,
    UserError::ConnectFailed,
    UserError::ConnectionLost,
    UserError::NothingNearby,
    UserError::PairingCodesDiffer,
    UserError::PairingDeclined,
    UserError::PairingExpired,
    UserError::TooManyTries,
    UserError::DeviceForgotten,
    UserError::CameraDenied,
    UserError::NotificationsOff,
    UserError::AccessibilityOff,
    UserError::ControlNotAllowed,
    UserError::SharingPartMissing,
    UserError::ScreenCaptureUnavailable,
    UserError::ShareCouldNotStart,
    UserError::StartupFailed,
    UserError::HostStoppedLocal,
    UserError::CannotShowSharedScreen,
];

/// A device platform used for platform-specific copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Use the common message.
    Any,
    /// A Windows device.
    Windows,
    /// A Linux device.
    Linux,
    /// An Android device.
    Android,
}

/// A user-facing message and optional technical detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMessage {
    /// Short, non-technical message title.
    pub title: &'static str,
    /// The single next action to take.
    pub next_step: &'static str,
    /// Technical text for an advanced details view.
    pub technical: Option<String>,
}

impl UserError {
    /// Returns the platform-independent message title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::NoNetwork => "This device isn't connected to Wi-Fi.",
            Self::SameWifi => "Couldn't find the other device on this Wi-Fi.",
            Self::HostUnreachable => "The other device didn't answer.",
            Self::ConnectionExpired => "This connection has expired or isn't valid.",
            Self::HostBusy => "The other device is busy.",
            Self::FirewallBlocked => "Something on this device blocked the connection.",
            Self::VideoRuntimeMissing => "Video support is missing.",
            Self::ScreenPermissionDenied => "Screen sharing wasn't allowed.",
            Self::ScreenCaptureBlocked => "This computer can't share its screen yet.",
            Self::ExtendUnsupported => "A second screen isn't available here.",
            Self::DecodeFailed => "Couldn't show the other screen.",
            Self::HostStopped => "The other device stopped sharing.",
            Self::VersionMismatch => "The other device uses a different version.",
            Self::Unknown => "Something went wrong.",
            Self::ConnectFailed => "Couldn't connect.",
            Self::ConnectionLost => "The connection was lost.",
            Self::NothingNearby => "No devices found nearby.",
            Self::PairingCodesDiffer => "The codes don't match.",
            Self::PairingDeclined => "The other device didn't accept.",
            Self::PairingExpired => "Pairing took too long.",
            Self::TooManyTries => "Too many tries.",
            Self::DeviceForgotten => "This device isn't paired any more.",
            Self::CameraDenied => "The camera is turned off for MightyMonitor.",
            Self::NotificationsOff => "Notifications are off.",
            Self::AccessibilityOff => "Remote control needs one more setting.",
            Self::ControlNotAllowed => "The other device hasn't allowed control.",
            Self::SharingPartMissing => "A part of MightyMonitor is missing.",
            Self::ScreenCaptureUnavailable => "This device can't share its screen.",
            Self::ShareCouldNotStart => "Sharing couldn't start.",
            Self::StartupFailed => "MightyMonitor couldn't start.",
            Self::HostStoppedLocal => "Sharing stopped.",
            Self::CannotShowSharedScreen => "This device can't show a shared screen.",
        }
    }

    /// Returns the platform-independent next step.
    pub const fn next_step(self) -> &'static str {
        match self {
            Self::NoNetwork => "Connect to your Wi-Fi, then try again.",
            Self::SameWifi => "Connect both devices to the same Wi-Fi, then try again.",
            Self::HostUnreachable => "Check that it is still sharing, then try again.",
            Self::ConnectionExpired => "On the other device, start sharing again, then try again.",
            Self::HostBusy => "Wait a moment, then try again.",
            Self::FirewallBlocked => "Allow MightyMonitor when your system asks, then try again.",
            Self::VideoRuntimeMissing => "Run the MightyMonitor installer again.",
            Self::ScreenPermissionDenied => "Try again and choose Allow when asked.",
            Self::ScreenCaptureBlocked => {
                "Sign out, choose Xorg or X11 on the sign-in screen, then sign in."
            }
            Self::ExtendUnsupported => "Share this screen instead.",
            Self::DecodeFailed => "Disconnect, then connect again.",
            Self::HostStopped => "Ask it to start sharing again.",
            Self::VersionMismatch => "Update MightyMonitor on both devices.",
            Self::Unknown => "Close MightyMonitor and open it again.",
            Self::ConnectFailed => "Check that both devices use the same Wi-Fi, then try again.",
            Self::ConnectionLost => "Keep both devices on the same Wi-Fi while we try again.",
            Self::NothingNearby => "On the other device, press Share this screen.",
            Self::PairingCodesDiffer => "Press No, then start pairing again.",
            Self::PairingDeclined => "Try again, and ask the other person to press Yes.",
            Self::PairingExpired => "Start again. You have one minute to finish.",
            Self::TooManyTries => "Wait a minute, then try again.",
            Self::DeviceForgotten => "Pair it again to continue.",
            Self::CameraDenied => "Turn it on in your phone's Settings.",
            Self::NotificationsOff => "Turn them on in Settings so you can stop sharing easily.",
            Self::AccessibilityOff => "Turn on MightyMonitor in Accessibility settings.",
            Self::ControlNotAllowed => "Ask the owner to turn on remote control.",
            Self::SharingPartMissing => "Reinstall MightyMonitor.",
            Self::ScreenCaptureUnavailable => {
                "Use a Windows or Linux computer, or an Android phone, to share."
            }
            Self::ShareCouldNotStart => "Close other screen-sharing apps, then try again.",
            Self::StartupFailed => "Run the installer again to repair it.",
            Self::HostStoppedLocal => "Press Share this screen to start again.",
            Self::CannotShowSharedScreen => {
                "Use a Windows or Linux computer, or an Android phone, to view."
            }
        }
    }

    /// Builds the message for a platform, without exposing technical details.
    pub fn message(self, platform: Platform) -> UserMessage {
        let (title, next_step) = match (self, platform) {
            (Self::FirewallBlocked, Platform::Windows) => (
                "Windows blocked the connection.",
                "Allow MightyMonitor in Windows Security, then try again.",
            ),
            (Self::VideoRuntimeMissing, Platform::Linux) => {
                (self.title(), "Reinstall MightyMonitor.")
            }
            (Self::ScreenPermissionDenied, Platform::Android) => {
                (self.title(), "Try again and choose Start now when asked.")
            }
            _ => (self.title(), self.next_step()),
        };

        UserMessage {
            title,
            next_step,
            technical: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Platform, UserError, ALL};
    use crate::strings::{Audience, ALL as UI_STRINGS};

    const BANNED: &[&str] = &[
        "IP",
        "port",
        "TCP",
        "UDP",
        "TLS",
        "SSL",
        "certificate",
        "fingerprint",
        "token",
        "codec",
        "H.264",
        "bitrate",
        "kbit",
        "fps",
        "session id",
        "handshake",
        "socket",
        "bind",
        "listener",
        "mDNS",
        "RTT",
        "ABR",
        "endpoint",
        "legacy",
        "unencrypted",
        "invitation",
        "payload",
        "protocol",
        "link",
        "address",
        "runtime",
        "driver",
        "package",
        "portal",
        "Wayland",
        "PipeWire",
        "GStreamer",
        "SHA",
        "firewall",
        "VPN",
    ];

    fn contains_banned_word(text: &str) -> bool {
        let lower = text.to_ascii_lowercase();
        BANNED.iter().any(|word| {
            let word = word.to_ascii_lowercase();
            lower.match_indices(&word).any(|(start, found)| {
                let before_is_word = lower[..start].chars().next_back().is_some_and(is_word_char);
                let after_is_word = lower[start + found.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_word_char);
                !before_is_word && !after_is_word
            })
        })
    }

    fn is_word_char(character: char) -> bool {
        character.is_alphanumeric() || character == '-'
    }

    #[test]
    fn jargon_free_default_strings() {
        for string in UI_STRINGS
            .iter()
            .filter(|string| string.audience == Audience::Default)
        {
            assert!(
                !contains_banned_word(string.text),
                "default string {:?} contains a banned word: {:?}",
                string.key,
                string.text
            );
        }
    }

    #[test]
    fn every_user_error_has_plain_message_and_next_step() {
        let platforms = [
            Platform::Any,
            Platform::Windows,
            Platform::Linux,
            Platform::Android,
        ];

        for error in ALL {
            for platform in platforms {
                let message = error.message(platform);
                for text in [message.title, message.next_step] {
                    assert!(!text.is_empty(), "{error:?} has empty copy");
                    assert!(text.len() <= 90, "{error:?} copy is too long: {text}");
                    assert!(
                        !contains_banned_word(text),
                        "{error:?} copy contains a banned word: {text}"
                    );
                }
                assert!(
                    message.next_step.ends_with('.'),
                    "{error:?} next step must end with a period"
                );
            }
        }
    }

    #[test]
    fn platform_variants_match_catalog() {
        let windows = UserError::FirewallBlocked.message(Platform::Windows);
        assert_eq!(windows.title, "Windows blocked the connection.");
        assert_eq!(
            windows.next_step,
            "Allow MightyMonitor in Windows Security, then try again."
        );

        let linux = UserError::VideoRuntimeMissing.message(Platform::Linux);
        assert_eq!(linux.title, UserError::VideoRuntimeMissing.title());
        assert_eq!(linux.next_step, "Reinstall MightyMonitor.");

        let android = UserError::ScreenPermissionDenied.message(Platform::Android);
        assert_eq!(android.title, UserError::ScreenPermissionDenied.title());
        assert_eq!(
            android.next_step,
            "Try again and choose Start now when asked."
        );
    }
}
