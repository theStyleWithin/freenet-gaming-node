//! Freenet contract wrapper around `lobby-types`.
//!
//! All rules live in `lobby-types` (natively testable). This file only
//! translates between Freenet's byte-oriented `ContractInterface` and those
//! types. Parameters = JSON `LobbyParameters` (one contract per game).
//! State/delta/summary = JSON.

use freenet_stdlib::prelude::*;
use lobby_types::{LobbyParameters, LobbyState, LobbySummary};

pub struct Contract;

fn params(p: &Parameters<'_>) -> Result<LobbyParameters, ContractError> {
    serde_json::from_slice(p.as_ref()).map_err(|e| ContractError::Deser(e.to_string()))
}

fn state(bytes: &[u8]) -> Result<LobbyState, ContractError> {
    if bytes.is_empty() {
        return Ok(LobbyState::default());
    }
    serde_json::from_slice(bytes).map_err(|e| ContractError::Deser(e.to_string()))
}

#[contract]
impl ContractInterface for Contract {
    fn validate_state(
        parameters: Parameters<'static>,
        state_: State<'static>,
        _related: RelatedContracts<'static>,
    ) -> Result<ValidateResult, ContractError> {
        let p = params(&parameters)?;
        state(state_.as_ref())?
            .validate(&p)
            .map(|()| ValidateResult::Valid)
            .map_err(|_| ContractError::InvalidState)
    }

    fn update_state(
        parameters: Parameters<'static>,
        state_: State<'static>,
        data: Vec<UpdateData<'static>>,
    ) -> Result<UpdateModification<'static>, ContractError> {
        let p = params(&parameters)?;
        let mut current = state(state_.as_ref())?;
        for ud in data {
            // State, Delta and StateAndDelta all carry LobbyState JSON.
            let chunks: Vec<Vec<u8>> = match ud {
                UpdateData::State(s) => vec![s.as_ref().to_vec()],
                UpdateData::Delta(d) => vec![d.as_ref().to_vec()],
                UpdateData::StateAndDelta { state, delta } => {
                    vec![state.as_ref().to_vec(), delta.as_ref().to_vec()]
                }
                _ => continue, // related-contract data: unused for now
            };
            for c in chunks.into_iter().filter(|c| !c.is_empty()) {
                let incoming = state(&c)?;
                current
                    .merge(incoming.announcements, &p)
                    .map_err(|_| ContractError::InvalidUpdate)?;
            }
        }
        let bytes = serde_json::to_vec(&current).map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(UpdateModification::valid(State::from(bytes)))
    }

    fn summarize_state(
        _parameters: Parameters<'static>,
        state_: State<'static>,
    ) -> Result<StateSummary<'static>, ContractError> {
        let s = state(state_.as_ref())?.summary();
        let bytes = serde_json::to_vec(&s).map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateSummary::from(bytes))
    }

    fn get_state_delta(
        _parameters: Parameters<'static>,
        state_: State<'static>,
        summary: StateSummary<'static>,
    ) -> Result<StateDelta<'static>, ContractError> {
        let sum: LobbySummary = if summary.as_ref().is_empty() {
            LobbySummary::default()
        } else {
            serde_json::from_slice(summary.as_ref()).map_err(|e| ContractError::Deser(e.to_string()))?
        };
        let delta = state(state_.as_ref())?.delta_for(&sum);
        let bytes = serde_json::to_vec(&delta).map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateDelta::from(bytes))
    }
}
