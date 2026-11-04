//! The only place the Gaming Node should touch Freenet.
//!
//! Freenet is alpha and its API moves, so everything else depends on the
//! [`LobbyDirectory`] trait. Phase 3 adds an implementation backed by the local
//! node's WebSocket API (loopback only, port 7509) and the `lobby-contract` WASM.

use lobby_types::{Announcement, LobbyError, LobbyParameters, LobbyState};

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    Lobby(#[from] LobbyError),
}

pub trait LobbyDirectory {
    fn publish(&mut self, a: Announcement) -> Result<(), AdapterError>;
    /// Live lobbies at `now` (unix secs).
    fn list(&self, now: u64) -> Vec<Announcement>;
}

/// In-memory stand-in so Phases 1-2 can run with no Freenet node.
pub struct InMemoryDirectory {
    params: LobbyParameters,
    state: LobbyState,
}

impl InMemoryDirectory {
    pub fn new(game_id: &str) -> Self {
        Self { params: LobbyParameters { game_id: game_id.into() }, state: LobbyState::default() }
    }
}

impl LobbyDirectory for InMemoryDirectory {
    fn publish(&mut self, a: Announcement) -> Result<(), AdapterError> {
        Ok(self.state.merge([a], &self.params)?)
    }
    fn list(&self, now: u64) -> Vec<Announcement> {
        self.state.active(now).cloned().collect()
    }
}
