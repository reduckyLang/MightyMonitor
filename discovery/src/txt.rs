//! A closed advertisement schema: no invitation or credential input exists.
use std::{collections::BTreeMap, io};
/// DNS-SD service type for untrusted MightyMonitor routes.
pub const SERVICE: &str = "_mightymonitor._tcp.local.";
/// Only keys emitted by the advertisement builder.
pub const KEYS: [&str; 3] = ["name", "id", "wire"];
/// Supported MightyMonitor wire version.
pub const WIRE: &str = "2";
/// Keep at most 40 safe ASCII characters, using a neutral fallback.
pub fn sanitize_name(name: &str) -> String {
    let result: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .take(40)
        .collect();
    let result = result.trim();
    if result.is_empty() {
        "MightyMonitor".into()
    } else {
        result.into()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Validated public metadata, with no invitation or key fields.
pub struct Metadata {
    name: String,
    id: String,
}
impl Metadata {
    /// Sanitized display name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Caller-supplied short opaque routing identifier, not proof of identity.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Build metadata with a nonempty lowercase hex ID of at most 16 characters.
    pub fn new(name: &str, id: &str) -> io::Result<Self> {
        if id.is_empty()
            || id.len() > 16
            || !id
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid short device id",
            ));
        }
        Ok(Self {
            name: sanitize_name(name),
            id: id.into(),
        })
    }
    /// Build a bounded DNS-SD instance name.
    pub fn instance(&self) -> String {
        format!("{}-{}", self.name, self.id)
    }
    /// Build a bounded local hostname from the opaque ID.
    pub fn hostname(&self) -> String {
        format!("mm-{}.local.", self.id)
    }
    /// Build the allow-listed TXT records, without any credentials.
    pub fn records(&self) -> BTreeMap<String, String> {
        [
            ("name", self.name.as_str()),
            ("id", self.id.as_str()),
            ("wire", WIRE),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
}
/// Parse bounded UTF-8 TXT records; reject duplicates and invalid versions, ignoring unknown keys.
pub fn parse<'a>(records: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> io::Result<Metadata> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid discovery metadata");
    let mut fields = BTreeMap::new();
    let mut total = 0;
    let mut seen = std::collections::BTreeSet::new();
    for (key, bytes) in records {
        if key.is_empty()
            || key.len() > 32
            || bytes.len() > 128
            || !key.bytes().all(|b| b.is_ascii_graphic() && b != b'=')
        {
            return Err(invalid());
        }
        total += key.len() + bytes.len() + 1;
        if total > 1024 {
            return Err(invalid());
        }
        let value = std::str::from_utf8(bytes).map_err(|_| invalid())?;
        let key = key.to_ascii_lowercase();
        if !seen.insert(key.clone()) {
            return Err(invalid());
        }
        if !KEYS.contains(&key.as_str()) {
            continue;
        }
        if fields.insert(key, value).is_some() {
            return Err(invalid());
        }
    }
    if fields.get("wire") != Some(&WIRE) {
        return Err(invalid());
    }
    let name = fields.get("name").ok_or_else(invalid)?;
    let id = fields.get("id").ok_or_else(invalid)?;
    if sanitize_name(name) != *name {
        return Err(invalid());
    }
    Metadata::new(name, id).map_err(|_| invalid())
}
