use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{block::L2BlockExecutionData, commitment::PubdataParams, H256};
use zksync_vm_interface::{L1BatchEnv, SystemEnv};

/// Wire-format mirror of `zksync_types::commitment::BlobHash`.
///
/// Field-for-field identical to the upstream type and to the verifier's
/// `crates/types/src/commitment::BlobHash`; defined locally because the
/// upstream struct only derives `Serialize`/`Deserialize` under `cfg(test)`.
/// Bincode-encodes as two `H256` in declaration order, matching the verifier's
/// expectation.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlobHash {
    pub commitment: H256,
    pub linear_hash: H256,
}

/// Version 1 of the data used as input for the airbender verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V1AirbenderVerifierInput {
    pub vm_run_data: VMRunWitnessInputData,
    pub merkle_paths: WitnessInputMerklePaths,
    pub l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
    pub pubdata_params: PubdataParams,
}

/// L1-settlement-bound data needed to verify the batch commitment chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitmentInput {
    /// `commitment` field of the previous (N-1) L1 batch.
    pub prev_batch_commitment: H256,
    /// `meta_parameters_hash` of the previous L1 batch.
    pub prev_meta_hash: H256,
    /// `aux_data_hash` of the previous L1 batch.
    pub prev_aux_hash: H256,
    /// One entry per EIP-4844 blob slot. Empty slots zero.
    pub blob_hashes: Vec<BlobHash>,
    /// One EIP-4844 versioned hash per blob slot.
    pub blob_versioned_hashes: Vec<H256>,
}

/// Version 2 of the verifier input: V1 plus the L1 commitment chain context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V2AirbenderVerifierInput {
    pub v1: V1AirbenderVerifierInput,
    pub commitment_input: CommitmentInput,
}

/// Data used as input for the airbender verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum AirbenderVerifierInput {
    V2(V2AirbenderVerifierInput),
}
