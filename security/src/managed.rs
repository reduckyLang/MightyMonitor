//! Bounded local Share commands and status. Never include credentials in status.
use std::io::{self, BufRead};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Stop,
    Confirm,
    Deny,
    Renew,
    Forget(String),
}
impl Command {
    pub fn parse(line: &str) -> Option<Self> {
        match line {
            "STOP" => Some(Self::Stop),
            "CONFIRM_PAIR" => Some(Self::Confirm),
            "DENY_PAIR" => Some(Self::Deny),
            "RENEW_PAIR" => Some(Self::Renew),
            _ => line
                .strip_prefix("FORGET ")
                .filter(|id| valid_id(id))
                .map(|id| Self::Forget(id.into())),
        }
    }
    pub fn line(&self) -> String {
        match self {
            Self::Stop => "STOP".into(),
            Self::Confirm => "CONFIRM_PAIR".into(),
            Self::Deny => "DENY_PAIR".into(),
            Self::Renew => "RENEW_PAIR".into(),
            Self::Forget(id) => format!("FORGET {id}"),
        }
    }
}
fn valid_id(id: &str) -> bool {
    id.len() == 16
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Open(u64),
    Locked,
    Expired,
    Code(String),
}
impl Status {
    pub fn parse(line: &str) -> Option<Self> {
        match line {
            "MM_PAIR LOCKED" => Some(Self::Locked),
            "MM_PAIR EXPIRED" => Some(Self::Expired),
            _ => {
                if let Some(value) = line.strip_prefix("MM_PAIR OPEN ") {
                    if value.len() <= 2 && value.bytes().all(|b| b.is_ascii_digit()) {
                        return value
                            .parse::<u64>()
                            .ok()
                            .filter(|n| *n <= 60)
                            .map(Self::Open);
                    }
                }
                line.strip_prefix("MM_PAIR_CODE ")
                    .filter(|code| code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()))
                    .map(|code| Self::Code(code.into()))
            }
        }
    }
}
/// Reject an overlong line before allocating more than the fixed limit.
/// EOF, malformed UTF-8 and overlong input terminate the command channel.
pub fn read_line(input: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::with_capacity(128);
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::other("Incomplete command"))
            };
        }
        let n = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |n| n + 1);
        if bytes.len() + n > 1024 {
            return Err(io::Error::other("Command too long"));
        }
        bytes.extend_from_slice(&available[..n]);
        input.consume(n);
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            return String::from_utf8(bytes).map(Some).map_err(io::Error::other);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_commands_status_and_bounds() {
        for line in [
            "STOP",
            "CONFIRM_PAIR",
            "DENY_PAIR",
            "RENEW_PAIR",
            "FORGET 0123456789abcdef",
        ] {
            assert_eq!(Command::parse(line).unwrap().line(), line);
        }
        for line in [
            "STOP\nFORGET 0123456789abcdef",
            "STOP\r",
            " CONFIRM_PAIR",
            "FORGET ../x",
            "FORGET 0123456789ABCDEF",
            "RENEW_PAIR ",
        ] {
            assert!(Command::parse(line).is_none());
        }
        for line in [
            "MM_PAIR OPEN 60",
            "MM_PAIR LOCKED",
            "MM_PAIR EXPIRED",
            "MM_PAIR_CODE 012345",
        ] {
            assert!(Status::parse(line).is_some());
        }
        for line in [
            "MM_PAIR OPEN 61",
            "MM_PAIR OPEN +1",
            "MM_PAIR_CODE 12345",
            "MM_PAIR_CODE 123456\nSTOP",
        ] {
            assert!(Status::parse(line).is_none());
        }
        assert!(read_line(&mut &b"STOP\n"[..]).unwrap().is_some());
        assert!(read_line(&mut &b""[..]).unwrap().is_none());
        assert!(read_line(&mut &b"STOP"[..]).is_err());
        assert!(read_line(&mut &vec![b'a'; 1025][..]).is_err());
    }
}
