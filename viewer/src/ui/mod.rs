//! Pure bounded presentation state; route and credential data stays elsewhere.
use std::sync::Arc;
use trinix_security::pairing::{sanitize_name, Clock};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NearbyRow {
    pub name: String,
    pub paired: bool,
}
#[derive(Default)]
pub struct NearbyModel {
    pub rows: Vec<NearbyRow>,
    pub searching: bool,
}
impl NearbyModel {
    pub fn replace(&mut self, names: impl IntoIterator<Item = (String, bool)>) {
        self.rows.clear();
        for (name, paired) in names.into_iter().take(64) {
            let name = sanitize_name(&name);
            let count = self
                .rows
                .iter()
                .filter(|row| row.name == name || row.name.starts_with(&format!("{name} (")))
                .count();
            self.rows.push(NearbyRow {
                name: if count == 0 {
                    name
                } else {
                    format!("{name} ({})", count + 1)
                },
                paired,
            });
        }
        self.searching = false;
    }
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub enum PairFlow {
    #[default]
    Idle,
    Check(String),
    Waiting(String),
    Paired,
    Failed,
}
impl PairFlow {
    pub fn code(&mut self, code: &str) -> bool {
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            *self = Self::Failed;
            return false;
        }
        *self = Self::Check(code.into());
        true
    }
    pub fn confirm(&mut self, matches: bool) {
        *self = match (&self, matches) {
            (Self::Check(code), true) => Self::Waiting(code.clone()),
            _ => Self::Failed,
        };
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum WindowState {
    Open { expires_in: u64 },
    Locked,
    Expired,
    Pairing { code: String },
}
pub struct HostPairingWindow {
    clock: Arc<dyn Clock>,
    end: u64,
    failures: u8,
    code: Option<String>,
}
impl HostPairingWindow {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        let end = clock.now().saturating_add(60);
        Self {
            clock,
            end,
            failures: 0,
            code: None,
        }
    }
    pub fn state(&self) -> WindowState {
        if self.failures >= 3 {
            WindowState::Locked
        } else if self.clock.now() >= self.end {
            WindowState::Expired
        } else if let Some(code) = &self.code {
            WindowState::Pairing { code: code.clone() }
        } else {
            WindowState::Open {
                expires_in: self.end.saturating_sub(self.clock.now()),
            }
        }
    }
    pub fn show_code(&mut self, code: &str) -> bool {
        if matches!(self.state(), WindowState::Open { .. })
            && code.len() == 6
            && code.bytes().all(|b| b.is_ascii_digit())
        {
            self.code = Some(code.into());
            true
        } else {
            false
        }
    }
    pub fn fail(&mut self) {
        self.failures = self.failures.saturating_add(1);
        self.code = None;
    }
    pub fn renew(&mut self) {
        self.end = self.clock.now().saturating_add(60);
        self.failures = 0;
        self.code = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct FakeClock(AtomicU64);
    impl Clock for FakeClock {
        fn now(&self) -> u64 {
            self.0.load(Ordering::Relaxed)
        }
    }
    #[test]
    fn window_expiry_lockout_and_renew() {
        let clock = Arc::new(FakeClock(AtomicU64::new(10)));
        let mut window = HostPairingWindow::new(clock.clone());
        assert_eq!(window.state(), WindowState::Open { expires_in: 60 });
        for _ in 0..3 {
            window.fail();
        }
        assert_eq!(window.state(), WindowState::Locked);
        window.renew();
        clock.0.store(70, Ordering::Relaxed);
        assert_eq!(window.state(), WindowState::Expired);
    }
    #[test]
    fn names_and_pair_flow_are_display_only() {
        let mut nearby = NearbyModel::default();
        nearby.replace([
            ("192.168.1.2".into(), false),
            ("room:5000".into(), true),
            ("Room".into(), false),
            ("Room".into(), false),
        ]);
        assert_eq!(nearby.rows[0].name, "Device");
        assert_eq!(nearby.rows[1].name, "Device (2)");
        assert_eq!(nearby.rows[3].name, "Room (2)");
        let mut pair = PairFlow::default();
        assert!(!pair.code("12345\n"));
        assert!(pair.code("012345"));
        pair.confirm(true);
        assert_eq!(pair, PairFlow::Waiting("012345".into()));
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn every_display_label_rejects_route_patterns() {
        let routes = regex::Regex::new(r"\d+\.\d+\.\d+\.\d+|:\d{2,5}").unwrap();
        let inputs = [
            "192.168.1.2",
            "room 999.1.2.3",
            "device:5000",
            "[::1]:5000",
            "١.٢.٣.٤",
            "room:١٢",
            "Room",
            "Room",
            "a\n192.1.1.1\n",
        ];
        let mut model = NearbyModel::default();
        model.replace(inputs.iter().map(|value| (value.to_string(), true)));
        for row in &model.rows {
            for text in [row.name.as_str(), if row.paired { "Paired" } else { "New" }] {
                assert!(!routes.is_match(text));
            }
        }
        // The same sanitization boundary is used for persisted paired names.
        for name in inputs {
            assert!(!routes.is_match(&sanitize_name(name)));
        }
        for text in [
            "Pairing did not finish. Start again on both devices.",
            "Couldn't connect. Pair this device again.",
            "This device has changed. Pair it again.",
            "Nearby may not work on guest or shared Wi-Fi",
        ] {
            assert!(!routes.is_match(text));
        }
        model.replace((0..100).map(|_| ("Device".into(), false)));
        assert_eq!(model.rows.len(), 64);
    }
}
