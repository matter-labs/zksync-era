use near_primitives::{
    borsh::{BorshDeserialize, BorshSerialize},
    hash::CryptoHash,
    merkle::MerklePath,
    serialize::dec_format,
    types::{AccountId, Balance, BlockHeight, Gas},
    views::{
        BlockHeaderInnerLiteView as NearBlockHeaderInnerLiteView,
        ExecutionOutcomeView as NearExecutionOutcomeView,
        ExecutionOutcomeWithIdView as NearExecutionOutcomeWithIdView, ExecutionStatusView,
        LightClientBlockLiteView as NearLightClientBlockLiteView,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LatestHeaderResponse {
    pub result: String,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct BlobInclusionProof {
    pub outcome_proof: ExecutionOutcomeWithIdView,
    pub outcome_root_proof: MerklePath,
    pub block_header_lite: LightClientBlockLiteView,
    pub block_proof: MerklePath,
    pub head_merkle_root: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct LightClientBlockLiteView {
    pub prev_block_hash: CryptoHash,
    pub inner_rest_hash: CryptoHash,
    pub inner_lite: BlockHeaderInnerLiteView,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct BlockHeaderInnerLiteView {
    pub height: BlockHeight,
    pub epoch_id: CryptoHash,
    pub next_epoch_id: CryptoHash,
    pub prev_state_root: CryptoHash,
    pub outcome_root: CryptoHash,
    #[serde(with = "dec_format")]
    pub timestamp_nanosec: u64,
    pub next_bp_hash: CryptoHash,
    pub block_merkle_root: CryptoHash,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct ExecutionOutcomeWithIdView {
    pub proof: MerklePath,
    pub block_hash: CryptoHash,
    pub id: CryptoHash,
    pub outcome: ExecutionOutcomeView,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ExecutionOutcomeView {
    pub logs: Vec<String>,
    pub receipt_ids: Vec<CryptoHash>,
    pub gas_burnt: Gas,
    #[serde(with = "dec_format")]
    pub tokens_burnt: Balance,
    pub executor_id: AccountId,
    pub status: ExecutionStatusView,
}

impl TryFrom<NearLightClientBlockLiteView> for LightClientBlockLiteView {
    type Error = anyhow::Error;

    fn try_from(value: NearLightClientBlockLiteView) -> Result<Self, Self::Error> {
        Ok(LightClientBlockLiteView {
            prev_block_hash: value.prev_block_hash,
            inner_rest_hash: value.inner_rest_hash,
            inner_lite: value.inner_lite.try_into()?,
        })
    }
}

impl TryFrom<NearBlockHeaderInnerLiteView> for BlockHeaderInnerLiteView {
    type Error = anyhow::Error;

    fn try_from(value: NearBlockHeaderInnerLiteView) -> Result<Self, Self::Error> {
        Ok(BlockHeaderInnerLiteView {
            height: value.height,
            epoch_id: value.epoch_id,
            next_epoch_id: value.next_epoch_id,
            prev_state_root: value.prev_state_root,
            outcome_root: value.outcome_root,
            timestamp_nanosec: value.timestamp_nanosec,
            next_bp_hash: value.next_bp_hash,
            block_merkle_root: value.block_merkle_root,
        })
    }
}

impl TryFrom<NearExecutionOutcomeWithIdView> for ExecutionOutcomeWithIdView {
    type Error = anyhow::Error;

    fn try_from(value: NearExecutionOutcomeWithIdView) -> Result<Self, Self::Error> {
        Ok(ExecutionOutcomeWithIdView {
            proof: value.proof,
            block_hash: value.block_hash,
            id: value.id,
            outcome: value.outcome.try_into()?,
        })
    }
}

impl TryFrom<NearExecutionOutcomeView> for ExecutionOutcomeView {
    type Error = anyhow::Error;

    fn try_from(value: NearExecutionOutcomeView) -> Result<Self, Self::Error> {
        Ok(ExecutionOutcomeView {
            logs: value.logs,
            receipt_ids: value.receipt_ids,
            gas_burnt: value.gas_burnt,
            tokens_burnt: value.tokens_burnt,
            executor_id: value.executor_id,
            status: value.status,
        })
    }
}
