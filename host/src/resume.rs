//! Bounded, expiring host session bindings; no historical video or input is kept.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;
use trinix_security::{random_session_id, AuthenticatedPeer};
const LIMIT: usize = 16;
const TTL: Duration = Duration::from_secs(300);
struct Entry {
    id: u64,
    peer: AuthenticatedPeer,
    sent: u64,
    expires: Instant,
}
#[derive(Clone, Default)]
pub struct ResumeRegistry(Arc<Mutex<VecDeque<Entry>>>);
impl ResumeRegistry {
    pub fn create(&self, peer: &AuthenticatedPeer) -> std::io::Result<u64> {
        let mut entries = self.0.lock().unwrap_or_else(|p| p.into_inner());
        entries.retain(|e| e.expires > Instant::now());
        let id = loop {
            let id = random_session_id()?;
            if !entries.iter().any(|e| e.id == id) {
                break id;
            }
        };
        if entries.len() >= LIMIT {
            entries.pop_front();
        }
        entries.push_back(Entry {
            id,
            peer: peer.clone(),
            sent: 0,
            expires: Instant::now() + TTL,
        });
        Ok(id)
    }
    pub fn resume(&self, id: u64, peer: &AuthenticatedPeer, last: u64) -> Option<u64> {
        if !peer.is_current() {
            return None;
        }
        let entries = self.0.lock().unwrap_or_else(|p| p.into_inner());
        entries
            .iter()
            .find(|e| e.id == id && &e.peer == peer && e.expires > Instant::now() && last <= e.sent)
            .map(|e| e.sent)
    }
    pub fn sent(&self, id: u64, seq: u64) {
        let mut entries = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
            e.sent = e.sent.max(seq);
            e.expires = Instant::now() + TTL;
        }
    }
}
