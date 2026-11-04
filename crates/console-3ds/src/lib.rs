//! Nintendo 3DS module (works against real hardware or a Citra-family emulator).
//!
//! PHASE 0 STATUS: nothing here is verified. The hostname list is intentionally
//! EMPTY: it must come from captured traffic / public docs (Pretendo's NEX
//! notes), not guesses. Until then the node forwards every lookup.

use node_core::{AuthResponder, ConsoleModule, DnsPolicy, NodeError, SessionManager};
use std::net::Ipv4Addr;

/// Hostnames to hijack. TODO(phase-0): fill from a Wireshark capture of MK7.
const HIJACKED_HOSTS: &[&str] = &[];

pub struct Mk7Dns;
impl DnsPolicy for Mk7Dns {
    fn resolve(&self, hostname: &str, local: Ipv4Addr) -> Option<Ipv4Addr> {
        let h = hostname.trim_end_matches('.').to_ascii_lowercase();
        HIJACKED_HOSTS.iter().any(|x| *x == h).then_some(local)
    }
}

pub struct Mk7Auth;
impl AuthResponder for Mk7Auth {
    fn respond(&self, _request: &[u8]) -> Result<Vec<u8>, NodeError> {
        Err(NodeError::Unsupported("MK7 auth stub not implemented (phase 1)".into()))
    }
}

pub struct Mk7Sessions;
impl SessionManager for Mk7Sessions {
    fn announcement_payload(&self) -> Vec<u8> {
        Vec::new()
    }
}

#[derive(Default)]
pub struct Nintendo3ds;
impl ConsoleModule for Nintendo3ds {
    fn name(&self) -> &'static str {
        "nintendo-3ds"
    }
    fn game_ids(&self) -> &'static [&'static str] {
        &["mk7"]
    }
    fn dns(&self) -> &dyn DnsPolicy {
        &Mk7Dns
    }
    fn auth(&self) -> &dyn AuthResponder {
        &Mk7Auth
    }
    fn sessions(&self) -> &dyn SessionManager {
        &Mk7Sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_everything_until_hostnames_are_researched() {
        let m = Nintendo3ds;
        assert_eq!(m.dns().resolve("example.com", Ipv4Addr::LOCALHOST), None);
        assert_eq!(m.game_ids(), &["mk7"]);
    }
}
