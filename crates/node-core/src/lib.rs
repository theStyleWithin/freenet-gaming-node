//! Console-agnostic core of the Gaming Node.
//!
//! Each console family implements [`ConsoleModule`]; the core never contains
//! console-specific knowledge. Phase 1 fills in DNS + auth; Phase 2 proves the
//! abstraction with a second console.

use std::net::Ipv4Addr;

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("unsupported request: {0}")]
    Unsupported(String),
}

/// Answers DNS names for one console family.
pub trait DnsPolicy {
    /// `Some(ip)` = answer with the Gaming Node's address (`local`);
    /// `None` = not ours, forward to the real resolver.
    fn resolve(&self, hostname: &str, local: Ipv4Addr) -> Option<Ipv4Addr>;
}

/// Produces the minimum login/entitlement replies so the console reaches its
/// online menu. Request/response are raw bytes until Phase 0 maps the protocols.
pub trait AuthResponder {
    fn respond(&self, request: &[u8]) -> Result<Vec<u8>, NodeError>;
}

/// Lobby/session behaviour for one game family.
pub trait SessionManager {
    /// Opaque per-game announcement payload (see `lobby_types::Announcement`).
    fn announcement_payload(&self) -> Vec<u8>;
}

/// A console family plugin.
pub trait ConsoleModule {
    fn name(&self) -> &'static str;
    /// Freenet lobby contract `game_id` per supported game.
    fn game_ids(&self) -> &'static [&'static str];
    fn dns(&self) -> &dyn DnsPolicy;
    fn auth(&self) -> &dyn AuthResponder;
    fn sessions(&self) -> &dyn SessionManager;
}
