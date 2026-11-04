//! Lobby data model and merge rules.
//!
//! This crate has **no Freenet dependency** so every rule can be unit-tested
//! natively. `lobby-contract` wraps it in the WASM `ContractInterface`.
//!
//! ## Model
//! One contract instance per game (the `game_id` is the contract parameter).
//! Its state is a set of [`Announcement`]s, each signed by the hosting node's
//! ed25519 key and identified by `(host_key, session_id)`.
//!
//! ## Why the contract never looks at a clock
//! Contract code must be deterministic, so it cannot ask "what time is it".
//! Expiry is therefore *data*: every announcement carries a signed
//! `issued_at`/`expires_at`. The contract only enforces a maximum lifetime
//! ([`MAX_TTL_SECS`]); readers drop expired entries with [`LobbyState::active`].
//! Stale entries are evicted by the size cap (soonest-to-expire first).
//!
//! ## Merge laws
//! `merge` must behave like a commutative, associative, idempotent join so
//! peers converge regardless of delivery order:
//! * per `(host_key, session_id)` the highest `seq` wins (ties: larger signature);
//! * the size cap keeps the top-N by a total order, which is associative.
//!
//! The tests below check these laws.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Domain-separation tag mixed into every signature.
pub const SIGN_DOMAIN: &[u8] = b"freenet-gaming-node/lobby/v1";
/// Max announcements retained per contract instance.
pub const MAX_ANNOUNCEMENTS: usize = 1024;
/// Max opaque payload bytes per announcement.
pub const MAX_PAYLOAD_BYTES: usize = 512;
/// Max lifetime (`expires_at - issued_at`) in seconds.
pub const MAX_TTL_SECS: u64 = 15 * 60;
/// Max game id length in bytes.
pub const MAX_GAME_ID_BYTES: usize = 64;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LobbyError {
    #[error("announcement is for game {got:?}, contract is for {expected:?}")]
    WrongGame { expected: String, got: String },
    #[error("game id too long")]
    GameIdTooLong,
    #[error("payload exceeds {MAX_PAYLOAD_BYTES} bytes")]
    PayloadTooLarge,
    #[error("expires_at must be after issued_at and within {MAX_TTL_SECS}s")]
    BadLifetime,
    #[error("malformed key or signature")]
    MalformedKey,
    #[error("signature does not verify")]
    BadSignature,
    #[error("state is not canonical (unsorted, duplicated or oversized)")]
    NotCanonical,
}

/// Contract parameters: which game this lobby list is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyParameters {
    pub game_id: String,
}

/// A host saying "I'm hosting / open for players" until `expires_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcement {
    pub game_id: String,
    /// ed25519 public key of the hosting node.
    pub host_key: [u8; 32],
    /// Host-chosen id, so one host can run several sessions.
    pub session_id: u64,
    /// Monotonic per `(host_key, session_id)`; higher replaces lower.
    pub seq: u64,
    /// Unix seconds, signed by the host (not checked against a clock here).
    pub issued_at: u64,
    pub expires_at: u64,
    /// Opaque game-specific info (player counts, region, ...). Do NOT put
    /// IP addresses here until Phase 4/5 decides how contact info is shared.
    pub payload: Vec<u8>,
    /// 64-byte ed25519 signature over [`Announcement::signing_bytes`].
    pub signature: Vec<u8>,
}

impl Announcement {
    pub fn key(&self) -> ([u8; 32], u64) {
        (self.host_key, self.session_id)
    }

    /// Canonical, unambiguous byte layout that is signed.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(SIGN_DOMAIN.len() + 128 + self.payload.len());
        b.extend_from_slice(SIGN_DOMAIN);
        b.extend_from_slice(&(self.game_id.len() as u32).to_le_bytes());
        b.extend_from_slice(self.game_id.as_bytes());
        b.extend_from_slice(&self.host_key);
        b.extend_from_slice(&self.session_id.to_le_bytes());
        b.extend_from_slice(&self.seq.to_le_bytes());
        b.extend_from_slice(&self.issued_at.to_le_bytes());
        b.extend_from_slice(&self.expires_at.to_le_bytes());
        b.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        b.extend_from_slice(&self.payload);
        b
    }

    /// Fill in `signature` using the host's key. `host_key` must match.
    pub fn sign(mut self, key: &SigningKey) -> Self {
        self.host_key = key.verifying_key().to_bytes();
        self.signature = key.sign(&self.signing_bytes()).to_bytes().to_vec();
        self
    }

    /// Stateless validity check (shape, lifetime bound, signature).
    pub fn verify(&self, params: &LobbyParameters) -> Result<(), LobbyError> {
        if self.game_id.len() > MAX_GAME_ID_BYTES {
            return Err(LobbyError::GameIdTooLong);
        }
        if self.game_id != params.game_id {
            return Err(LobbyError::WrongGame {
                expected: params.game_id.clone(),
                got: self.game_id.clone(),
            });
        }
        if self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(LobbyError::PayloadTooLarge);
        }
        if self.expires_at <= self.issued_at || self.expires_at - self.issued_at > MAX_TTL_SECS {
            return Err(LobbyError::BadLifetime);
        }
        let vk = VerifyingKey::from_bytes(&self.host_key).map_err(|_| LobbyError::MalformedKey)?;
        let sig_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| LobbyError::MalformedKey)?;
        vk.verify(&self.signing_bytes(), &Signature::from_bytes(&sig_bytes))
            .map_err(|_| LobbyError::BadSignature)
    }

    /// Total order used for tie-breaks and eviction: soonest-to-expire first.
    fn eviction_order(&self) -> (u64, [u8; 32], u64) {
        (self.expires_at, self.host_key, self.session_id)
    }
}

/// What a peer sends to describe what it already has.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbySummary {
    /// `(host_key, session_id, seq)` for every held announcement.
    pub entries: Vec<([u8; 32], u64, u64)>,
}

/// The contract state: announcements kept sorted by `(host_key, session_id)`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyState {
    pub announcements: Vec<Announcement>,
}

impl LobbyState {
    /// Check the whole state is valid and canonical.
    pub fn validate(&self, params: &LobbyParameters) -> Result<(), LobbyError> {
        if self.announcements.len() > MAX_ANNOUNCEMENTS {
            return Err(LobbyError::NotCanonical);
        }
        for w in self.announcements.windows(2) {
            if w[0].key() >= w[1].key() {
                return Err(LobbyError::NotCanonical);
            }
        }
        self.announcements.iter().try_for_each(|a| a.verify(params))
    }

    /// Merge `incoming` into `self`. Invalid announcements make the whole
    /// merge fail (the contract rejects the update) so bad data never lands.
    pub fn merge(
        &mut self,
        incoming: impl IntoIterator<Item = Announcement>,
        params: &LobbyParameters,
    ) -> Result<(), LobbyError> {
        let mut map: BTreeMap<([u8; 32], u64), Announcement> = std::mem::take(&mut self.announcements)
            .into_iter()
            .map(|a| (a.key(), a))
            .collect();
        for a in incoming {
            a.verify(params)?;
            match map.get(&a.key()) {
                Some(cur) if (cur.seq, &cur.signature) >= (a.seq, &a.signature) => {}
                _ => {
                    map.insert(a.key(), a);
                }
            }
        }
        let mut all: Vec<Announcement> = map.into_values().collect();
        if all.len() > MAX_ANNOUNCEMENTS {
            // Keep the MAX_ANNOUNCEMENTS latest-expiring; a total order makes
            // truncation associative/commutative across merge orders.
            all.sort_by_key(|a| std::cmp::Reverse(a.eviction_order()));
            all.truncate(MAX_ANNOUNCEMENTS);
            all.sort_by_key(|a| a.key());
        }
        self.announcements = all;
        Ok(())
    }

    pub fn summary(&self) -> LobbySummary {
        LobbySummary {
            entries: self
                .announcements
                .iter()
                .map(|a| (a.host_key, a.session_id, a.seq))
                .collect(),
        }
    }

    /// Announcements the peer described by `summary` is missing or has stale.
    pub fn delta_for(&self, summary: &LobbySummary) -> LobbyState {
        let known: BTreeMap<_, _> = summary
            .entries
            .iter()
            .map(|(h, s, q)| ((*h, *s), *q))
            .collect();
        LobbyState {
            announcements: self
                .announcements
                .iter()
                .filter(|a| known.get(&a.key()).is_none_or(|q| a.seq > *q))
                .cloned()
                .collect(),
        }
    }

    /// Client-side view: only announcements still live at `now` (unix secs).
    pub fn active(&self, now: u64) -> impl Iterator<Item = &Announcement> {
        self.announcements.iter().filter(move |a| a.expires_at > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn params() -> LobbyParameters {
        LobbyParameters { game_id: "mk7".into() }
    }

    fn ann(key: &SigningKey, session: u64, seq: u64, exp: u64) -> Announcement {
        Announcement {
            game_id: "mk7".into(),
            host_key: [0; 32],
            session_id: session,
            seq,
            issued_at: 1_000,
            expires_at: exp,
            payload: vec![1, 2, 3],
            signature: vec![],
        }
        .sign(key)
    }

    #[test]
    fn valid_announcement_verifies() {
        let k = SigningKey::generate(&mut OsRng);
        assert_eq!(ann(&k, 1, 1, 1_300).verify(&params()), Ok(()));
    }

    #[test]
    fn tampering_is_rejected() {
        let k = SigningKey::generate(&mut OsRng);
        let mut a = ann(&k, 1, 1, 1_300);
        a.payload.push(9);
        assert_eq!(a.verify(&params()), Err(LobbyError::BadSignature));
    }

    #[test]
    fn wrong_game_lifetime_and_payload_rejected() {
        let k = SigningKey::generate(&mut OsRng);
        let other = LobbyParameters { game_id: "other".into() };
        assert!(matches!(ann(&k, 1, 1, 1_300).verify(&other), Err(LobbyError::WrongGame { .. })));
        assert_eq!(ann(&k, 1, 1, 1_000 + MAX_TTL_SECS + 1).verify(&params()), Err(LobbyError::BadLifetime));
        assert_eq!(ann(&k, 1, 1, 1_000).verify(&params()), Err(LobbyError::BadLifetime));
        let mut big = ann(&k, 1, 1, 1_300);
        big.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(big.verify(&params()), Err(LobbyError::PayloadTooLarge));
    }

    #[test]
    fn higher_seq_wins_and_merge_is_commutative_and_idempotent() {
        let k1 = SigningKey::generate(&mut OsRng);
        let k2 = SigningKey::generate(&mut OsRng);
        let a1 = ann(&k1, 1, 1, 1_300);
        let a2 = ann(&k1, 1, 2, 1_400); // replaces a1
        let b = ann(&k2, 7, 1, 1_300);

        let mut x = LobbyState::default();
        x.merge([a1.clone(), b.clone()], &params()).unwrap();
        x.merge([a2.clone()], &params()).unwrap();

        let mut y = LobbyState::default();
        y.merge([a2.clone()], &params()).unwrap();
        y.merge([b.clone(), a1.clone()], &params()).unwrap();

        assert_eq!(x, y);
        assert_eq!(x.announcements.len(), 2);
        assert!(x.announcements.iter().any(|a| a.seq == 2));
        // idempotent
        let before = x.clone();
        x.merge(before.announcements.clone(), &params()).unwrap();
        assert_eq!(x, before);
        assert_eq!(x.validate(&params()), Ok(()));
    }

    #[test]
    fn invalid_announcement_fails_whole_merge() {
        let k = SigningKey::generate(&mut OsRng);
        let mut bad = ann(&k, 1, 1, 1_300);
        bad.seq = 99; // breaks signature
        let mut s = LobbyState::default();
        assert!(s.merge([ann(&k, 2, 1, 1_300), bad], &params()).is_err());
    }

    #[test]
    fn cap_evicts_soonest_expiring() {
        let mut s = LobbyState::default();
        let mut batch = Vec::new();
        for i in 0..(MAX_ANNOUNCEMENTS as u64 + 5) {
            let k = SigningKey::generate(&mut OsRng);
            batch.push(ann(&k, i, 1, 1_001 + (i % 600)));
        }
        s.merge(batch, &params()).unwrap();
        assert_eq!(s.announcements.len(), MAX_ANNOUNCEMENTS);
        assert_eq!(s.validate(&params()), Ok(()));
    }

    #[test]
    fn summary_and_delta_only_send_what_is_missing() {
        let k = SigningKey::generate(&mut OsRng);
        let mut a = LobbyState::default();
        a.merge([ann(&k, 1, 1, 1_300), ann(&k, 2, 1, 1_300)], &params()).unwrap();
        let mut b = LobbyState::default();
        b.merge([ann(&k, 1, 1, 1_300)], &params()).unwrap();
        let d = a.delta_for(&b.summary());
        assert_eq!(d.announcements.len(), 1);
        assert_eq!(d.announcements[0].session_id, 2);
        b.merge(d.announcements, &params()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn active_filters_expired() {
        let k = SigningKey::generate(&mut OsRng);
        let mut s = LobbyState::default();
        s.merge([ann(&k, 1, 1, 1_100), ann(&k, 2, 1, 1_500)], &params()).unwrap();
        assert_eq!(s.active(1_200).count(), 1);
    }
}
