use bellman::bn256::Bn256;
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuitNoLookupCustomGate;
use fflonk::FflonkVerificationKey;
use vise::{EncodeLabelSet, EncodeLabelValue};
use zksync_eth_client::{ContractCallError, EnrichedClientError, SigningError};
use zksync_types::{ethabi, H256, U256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "proving_network", rename_all = "snake_case")]
pub enum ProvingNetwork {
    None = 0,
    Fermah = 1,
    Lagrange = 2,
}

impl ProvingNetwork {
    pub fn from_u256(u: U256) -> anyhow::Result<Self> {
        match u.as_u32() {
            0 => Ok(Self::None),
            1 => Ok(Self::Fermah),
            2 => Ok(Self::Lagrange),
            _ => anyhow::bail!("unknown proving network discriminant: {}", u),
        }
    }
}

impl From<ProvingNetwork> for zksync_dal::eth_proof_manager_dal::ProvingNetwork {
    fn from(val: ProvingNetwork) -> Self {
        match val {
            ProvingNetwork::None => zksync_dal::eth_proof_manager_dal::ProvingNetwork::None,
            ProvingNetwork::Fermah => zksync_dal::eth_proof_manager_dal::ProvingNetwork::Fermah,
            ProvingNetwork::Lagrange => zksync_dal::eth_proof_manager_dal::ProvingNetwork::Lagrange,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProofRequestIdentifier {
    pub chain_id: u64,     // uint256
    pub block_number: u64, // uint256
}

impl ProofRequestIdentifier {
    pub fn into_tokens(self) -> ethabi::Token {
        ethabi::Token::Tuple(vec![
            ethabi::Token::Uint(U256::from(self.chain_id)),
            ethabi::Token::Uint(U256::from(self.block_number)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProofRequestParams {
    pub protocol_major: u32,      // uint32
    pub protocol_minor: u32,      // uint32
    pub protocol_patch: u32,      // uint32
    pub proof_inputs_url: String, // string
    pub timeout_after: u64,       // uint256
    pub max_reward: u64,          // uint256
}

impl ProofRequestParams {
    pub fn into_tokens(self) -> ethabi::Token {
        ethabi::Token::Tuple(vec![
            ethabi::Token::Uint(U256::from(self.protocol_major)),
            ethabi::Token::Uint(U256::from(self.protocol_minor)),
            ethabi::Token::Uint(U256::from(self.protocol_patch)),
            ethabi::Token::String(self.proof_inputs_url),
            ethabi::Token::Uint(U256::from(self.timeout_after)),
            ethabi::Token::Uint(U256::from(self.max_reward)),
        ])
    }
}

pub(crate) type FflonkFinalVerificationKey =
    FflonkVerificationKey<Bn256, ZkSyncSnarkWrapperCircuitNoLookupCustomGate>;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    ContractCall(#[from] ContractCallError),
    Ethabi(#[from] ethabi::Error),
    Signing(#[from] SigningError),
    Provider(#[from] EnrichedClientError),
    Generic(#[from] anyhow::Error),
    /// The transaction was mined and reverted.
    ///
    /// Unlike every other variant this is a verdict from the chain, not a transport
    /// failure: resending identical calldata reverts the same way, so callers must not
    /// treat it as retryable.
    Reverted {
        tx_hash: H256,
        /// Human-readable message from the node. Diagnostic only: a custom error leaves it
        /// empty, since there is no `Error(string)` to decode.
        revert_reason: String,
        /// Raw returndata of the revert, which is where a custom error's payload lives.
        revert_data: Vec<u8>,
    },
}

/// Mirrors `IProofManager.ProofRequestStatus`.
///
/// The contract lives in another repository, so the discriminants are transcribed rather
/// than generated. They were checked against the verified source of the implementation
/// deployed on Era mainnet at `0xf899e178da9666ad4833549ee525d65d0f689b19`
/// (`src/ProofManagerV1.sol:ProofManagerV1`, behind proxy
/// `0x7c168119bf754269523AC9728aeEba8e4Eee6100`). Re-check them when the contract is
/// upgraded: a wrong discriminant here can strand a batch whose reward is still locked.
///
/// Verify against the deployed source, not against a local checkout of the contracts —
/// checkouts around at the time of writing were older than mainnet (`onlyOwner` rather
/// than `onlyRole(SUBMITTER_ROLE)`, and no reward accounting at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofRequestStatus {
    PendingAcknowledgement,
    Committed,
    Refused,
    Unacknowledged,
    Proven,
    TimedOut,
    Validated,
    ValidationFailed,
}

impl ProofRequestStatus {
    pub fn from_u8(discriminant: u8) -> Option<Self> {
        Some(match discriminant {
            0 => Self::PendingAcknowledgement,
            1 => Self::Committed,
            2 => Self::Refused,
            3 => Self::Unacknowledged,
            4 => Self::Proven,
            5 => Self::TimedOut,
            6 => Self::Validated,
            7 => Self::ValidationFailed,
            _ => return None,
        })
    }

    /// The verdict the contract already recorded for the request, if any.
    ///
    /// These are exactly the two states `submitProofValidationResult` can leave a request
    /// in, and the deployed implementation decrements `potentialFutureReward` after the
    /// branch, so both settle the reward: nothing stays locked and no further call of ours
    /// can land. Every other state returns `None` and must stay retryable, since the
    /// reward may still be locked.
    pub fn recorded_verdict(self) -> Option<bool> {
        match self {
            Self::Validated => Some(true),
            Self::ValidationFailed => Some(false),
            _ => None,
        }
    }
}

/// 4-byte selector of `ProofRequestIsNotProven(ProofRequestStatus)`. Solidity ABI-encodes
/// the enum argument as a `uint8`. Pinned by `proof_request_is_not_proven_selector`.
const PROOF_REQUEST_IS_NOT_PROVEN_SELECTOR: [u8; 4] = [0x40, 0x20, 0x6e, 0x43];

/// Decodes a `ProofRequestIsNotProven(status)` revert out of raw returndata.
///
/// `None` only means we could not *prove* the revert was this error, and callers must fall
/// back to their normal retry path — never to abandoning the batch.
pub fn decode_proof_request_is_not_proven(revert_data: &[u8]) -> Option<ProofRequestStatus> {
    const WORD: usize = 32;

    if revert_data.len() < 4 + WORD || revert_data[..4] != PROOF_REQUEST_IS_NOT_PROVEN_SELECTOR {
        return None;
    }

    // The enum is ABI-encoded right-aligned in a 32-byte word; anything in the padding
    // means this is not the argument we think it is.
    let argument = &revert_data[4..4 + WORD];
    if argument[..WORD - 1].iter().any(|byte| *byte != 0) {
        return None;
    }

    ProofRequestStatus::from_u8(argument[WORD - 1])
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the hardcoded selector to the contract's error signature.
    #[test]
    fn proof_request_is_not_proven_selector() {
        assert_eq!(
            PROOF_REQUEST_IS_NOT_PROVEN_SELECTOR,
            ethabi::short_signature("ProofRequestIsNotProven", &[ethabi::ParamType::Uint(8)],)
        );
    }

    /// Encodes `ProofRequestIsNotProven(discriminant)` the way the contract would.
    fn revert_data(discriminant: u8) -> Vec<u8> {
        let mut data = PROOF_REQUEST_IS_NOT_PROVEN_SELECTOR.to_vec();
        data.extend_from_slice(&[0u8; 32]);
        *data.last_mut().unwrap() = discriminant;
        data
    }

    #[test]
    fn decodes_every_status() {
        let expected = [
            ProofRequestStatus::PendingAcknowledgement,
            ProofRequestStatus::Committed,
            ProofRequestStatus::Refused,
            ProofRequestStatus::Unacknowledged,
            ProofRequestStatus::Proven,
            ProofRequestStatus::TimedOut,
            ProofRequestStatus::Validated,
            ProofRequestStatus::ValidationFailed,
        ];
        for (discriminant, status) in expected.into_iter().enumerate() {
            assert_eq!(
                decode_proof_request_is_not_proven(&revert_data(discriminant as u8)),
                Some(status)
            );
        }
    }

    /// Only the two settled states may close a batch, and each carries the verdict the
    /// contract recorded; everything else keeps its reward locked and must stay retryable.
    #[test]
    fn only_settled_states_carry_a_verdict() {
        let expected = [
            (6u8, Some(true)), // Validated
            (7, Some(false)),  // ValidationFailed
            (0, None),
            (1, None),
            (2, None),
            (3, None),
            (4, None),
            (5, None),
        ];
        for (discriminant, verdict) in expected {
            let status = decode_proof_request_is_not_proven(&revert_data(discriminant)).unwrap();
            assert_eq!(status.recorded_verdict(), verdict, "{status:?}");
        }
    }

    /// Anything we cannot positively identify must stay retryable, never be abandoned.
    #[test]
    fn unknown_reverts_decode_to_none() {
        let mut truncated = revert_data(6);
        truncated.truncate(4 + 31);
        let mut dirty_padding = revert_data(6);
        dirty_padding[4] = 1;

        for data in [
            // Empty returndata, which is what a bare `execution reverted` carries.
            vec![],
            // `NoFundsAvailable()`, a different error with no arguments.
            vec![0xd2, 0xfe, 0xa1, 0xe0],
            PROOF_REQUEST_IS_NOT_PROVEN_SELECTOR.to_vec(),
            truncated,
            // Discriminant outside the enum.
            revert_data(8),
            dirty_padding,
        ] {
            assert_eq!(
                decode_proof_request_is_not_proven(&data),
                None,
                "{}",
                hex::encode(&data)
            );
        }
    }
}
