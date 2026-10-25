//! Default and advanced interface strings.

use crate::catalog;

/// Audience permitted to see an interface string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Suitable for the default interface.
    Default,
    /// Technical text shown only in an advanced section.
    Advanced,
}

/// One interface string with its stable key and audience.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiString {
    /// Stable identifier for the string.
    pub key: &'static str,
    /// Text displayed to the user.
    pub text: &'static str,
    /// Audience allowed to see this text.
    pub audience: Audience,
}

const UI_STRING_COUNT: usize = 11 + catalog::ALL.len() * 2;

const fn string(key: &'static str, text: &'static str, audience: Audience) -> UiString {
    UiString {
        key,
        text,
        audience,
    }
}

const fn build_all() -> [UiString; UI_STRING_COUNT] {
    let mut strings = [string("", "", Audience::Default); UI_STRING_COUNT];
    let mut index = 0;

    strings[index] = string(
        "connection.not_connected",
        "Not connected",
        Audience::Default,
    );
    index += 1;
    strings[index] = string("connection.connecting", "Connecting…", Audience::Default);
    index += 1;
    strings[index] = string(
        "connection.reconnecting",
        "Reconnecting…",
        Audience::Default,
    );
    index += 1;
    strings[index] = string("connection.connected", "Connected", Audience::Default);
    index += 1;
    strings[index] = string("sharing.starting", "Starting…", Audience::Default);
    index += 1;
    strings[index] = string("sharing.on", "Sharing is on", Audience::Default);
    index += 1;
    strings[index] = string(
        "viewer.empty",
        "The other screen will appear here.",
        Audience::Default,
    );
    index += 1;
    strings[index] = string("details.copy", "Copy details", Audience::Default);
    index += 1;

    let mut error_index = 0;
    while error_index < catalog::ALL.len() {
        let error = catalog::ALL[error_index];
        strings[index] = string(error.title(), error.title(), Audience::Default);
        index += 1;
        strings[index] = string(error.next_step(), error.next_step(), Audience::Default);
        index += 1;
        error_index += 1;
    }

    strings[index] = string("advanced.section", "Advanced", Audience::Advanced);
    index += 1;
    strings[index] = string(
        "advanced.connection_details",
        "Connection details",
        Audience::Advanced,
    );
    index += 1;
    strings[index] = string(
        "advanced.copy_technical_details",
        "Copy technical details",
        Audience::Advanced,
    );

    strings
}

/// All default and advanced interface strings, including catalog copy.
pub const ALL: &[UiString] = &build_all();
