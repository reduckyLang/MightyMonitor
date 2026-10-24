//! Opt-in LAN route discovery. All resolved peers remain untrusted.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
pub mod txt;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};
use txt::{Metadata, SERVICE};
/// Maximum retained services in a browse snapshot.
pub const MAX_RESULTS: usize = 64;
fn error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}
/// Small injectable lifecycle boundary. Implementations must bound event waits.
pub trait Backend {
    /// Register the public route metadata and return the service name.
    fn register(&mut self, metadata: &Metadata, address: SocketAddr) -> io::Result<String>;
    /// Unregister a service, with a bounded wait.
    fn unregister(&mut self, name: &str);
    /// Stop the backend daemon, with a bounded wait.
    fn shutdown(&mut self);
    /// Begin an on-demand browse.
    fn start_browse(&mut self) -> io::Result<()>;
    /// Receive at most one event within the supplied timeout.
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>>;
    /// Stop browsing and release its event receiver.
    fn stop_browse(&mut self);
}
#[derive(Clone, Debug)]
/// An untrusted route advertised by a nearby peer.
pub struct Device {
    /// Validated public display metadata.
    pub metadata: Metadata,
    /// Candidate network routes; these confer no trust.
    pub addresses: Vec<SocketAddr>,
}
/// A backend discovery update.
pub enum Event {
    /// A resolved service, keyed by its full service name.
    Resolved(String, Device),
    /// A service that disappeared.
    Removed(String),
}
/// Production mdns-sd backend; construction alone does not advertise.
pub struct Mdns {
    daemon: ServiceDaemon,
    events: Option<mdns_sd::Receiver<ServiceEvent>>,
}
impl Mdns {
    /// Create a daemon without starting advertisement or browsing.
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            daemon: ServiceDaemon::new().map_err(error)?,
            events: None,
        })
    }
}
impl Backend for Mdns {
    fn register(&mut self, metadata: &Metadata, address: SocketAddr) -> io::Result<String> {
        if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
            return Err(error("Use a specific LAN bind address"));
        }
        let properties: std::collections::HashMap<_, _> = metadata.records().into_iter().collect();
        let info = ServiceInfo::new(
            SERVICE,
            &metadata.instance(),
            &metadata.hostname(),
            address.ip(),
            address.port(),
            properties,
        )
        .map_err(error)?;
        let name = info.get_fullname().to_string();
        self.daemon.register(info).map_err(error)?;
        Ok(name)
    }
    fn unregister(&mut self, name: &str) {
        if let Ok(done) = self.daemon.unregister(name) {
            let _ = done.recv_timeout(Duration::from_millis(300));
        }
    }
    fn shutdown(&mut self) {
        if let Ok(done) = self.daemon.shutdown() {
            let _ = done.recv_timeout(Duration::from_millis(300));
        }
    }
    fn start_browse(&mut self) -> io::Result<()> {
        self.events = Some(self.daemon.browse(SERVICE).map_err(error)?);
        Ok(())
    }
    fn stop_browse(&mut self) {
        let _ = self.daemon.stop_browse(SERVICE);
        self.events = None;
    }
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        let Some(events) = &self.events else {
            return Err(error("Browse not started"));
        };
        match events.recv_timeout(timeout) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if info.get_fullname().len() > 128
                    || !info.get_fullname().ends_with(SERVICE)
                    || info.get_hostname().len() > 253
                    || info.get_port() == 0
                {
                    return Ok(None);
                }
                let Ok(metadata) = txt::parse(
                    info.get_properties()
                        .iter()
                        .map(|p| (p.key(), p.val().unwrap_or_default())),
                ) else {
                    return Ok(None);
                };
                let addresses = info
                    .get_addresses()
                    .iter()
                    .filter(|ip| {
                        !ip.is_unspecified()
                            && !ip.is_multicast()
                            && !matches!(ip,IpAddr::V6(v) if v.is_unicast_link_local())
                    })
                    .take(8)
                    .map(|ip| SocketAddr::new(*ip, info.get_port()))
                    .collect::<Vec<_>>();
                if addresses.is_empty() {
                    return Ok(None);
                }
                Ok(Some(Event::Resolved(
                    info.get_fullname().into(),
                    Device {
                        metadata,
                        addresses,
                    },
                )))
            }
            Ok(ServiceEvent::ServiceRemoved(_, name)) if name.len() <= 128 => {
                Ok(Some(Event::Removed(name)))
            }
            _ => Ok(None),
        }
    }
}
impl Drop for Mdns {
    fn drop(&mut self) {
        self.shutdown();
    }
}
/// Opt-in registration guard; dropping unregisters and shuts down the backend.
pub struct Advertisement<B: Backend> {
    backend: B,
    name: String,
}
impl<B: Backend> Advertisement<B> {
    /// Explicitly advertise metadata until the returned guard is dropped.
    pub fn start(mut backend: B, metadata: &Metadata, address: SocketAddr) -> io::Result<Self> {
        let name = match backend.register(metadata, address) {
            Ok(name) => name,
            Err(error) => {
                backend.shutdown();
                return Err(error);
            }
        };
        Ok(Self { backend, name })
    }
}
impl<B: Backend> Drop for Advertisement<B> {
    fn drop(&mut self) {
        self.backend.unregister(&self.name);
        self.backend.shutdown();
    }
}
#[derive(Default)]
/// Bounded, deduplicated collection with stale-service expiry.
pub struct Nearby {
    entries: BTreeMap<String, (Device, Instant)>,
}
impl Nearby {
    /// Apply an update, retaining at most MAX_RESULTS service names.
    pub fn apply(&mut self, event: Event, now: Instant) {
        match event {
            Event::Resolved(name, device) => {
                if name.len() <= 128
                    && (self.entries.contains_key(&name) || self.entries.len() < MAX_RESULTS)
                {
                    self.entries.insert(name, (device, now));
                }
            }
            Event::Removed(name) => {
                self.entries.remove(&name);
            }
        }
    }
    /// Remove services unseen for 120 seconds.
    pub fn expire(&mut self, now: Instant) {
        self.entries
            .retain(|_, (_, seen)| now.saturating_duration_since(*seen) < Duration::from_secs(120));
    }
    /// Return a snapshot of the retained untrusted routes.
    pub fn devices(&self) -> Vec<Device> {
        self.entries.values().map(|(d, _)| d.clone()).collect()
    }
}
/// Browse for at most 30 seconds, stopping on completion or error.
pub fn browse<B: Backend>(backend: &mut B, duration: Duration) -> io::Result<Vec<Device>> {
    if duration > Duration::from_secs(30) {
        return Err(error("Browse window too long"));
    }
    backend.start_browse()?;
    let result = (|| {
        let end = Instant::now() + duration;
        let mut nearby = Nearby::default();
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            if let Some(event) = backend.next(left.min(Duration::from_millis(100)))? {
                nearby.apply(event, Instant::now());
            }
            nearby.expire(Instant::now());
        }
        Ok(nearby.devices())
    })();
    backend.stop_browse();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires local multicast support; explicitly run on a test machine"]
    fn real_mdns_loopback() {
        let mut browser = Mdns::new().unwrap();
        let advertiser = Mdns::new().unwrap();
        for daemon in [&browser.daemon, &advertiser.daemon] {
            daemon.disable_interface(mdns_sd::IfKind::All).unwrap();
            daemon
                .enable_interface(mdns_sd::IfKind::LoopbackV4)
                .unwrap();
            daemon.set_multicast_loop_v4(true).unwrap();
        }
        let metadata = Metadata::new("Loopback test", "0123456789abcdef").unwrap();
        let _ad = Advertisement::start(advertiser, &metadata, "127.0.0.1:45001".parse().unwrap())
            .unwrap();
        let devices = browse(&mut browser, Duration::from_secs(5)).unwrap();
        assert!(devices.iter().any(|device| device.metadata == metadata));
    }
}
