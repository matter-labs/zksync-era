use serde::{Deserialize, Serialize};
use zksync_types::{
    block::UnsealedL1BatchHeader, fee_model::BatchFeeInput, settlement::SettlementLayer, Address,
    L1BatchNumber, ProtocolVersionId, H256,
};

use super::L2BlockEnv;

/// Unique params for each L1 batch.
///
/// Eventually, most of these parameters (`l1_gas_price`, `fair_l2_gas_price`, `fee_account`,
/// `enforced_base_fee`) will be moved to [`L2BlockEnv`]. For now, the VM doesn't support changing
/// them in the middle of execution; that's why these params are specified here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchEnv {
    // If previous batch hash is None, then this is the first batch
    pub previous_batch_hash: Option<H256>,
    pub number: L1BatchNumber,
    pub timestamp: u64,

    /// The fee input into the batch. It contains information such as L1 gas price, L2 fair gas price, etc.
    pub fee_input: BatchFeeInput,
    pub fee_account: Address,
    pub enforced_base_fee: Option<u64>,
    pub first_l2_block: L2BlockEnv,
    pub settlement_layer: SettlementLayer,
}

impl L1BatchEnv {
    pub fn into_unsealed_header(
        self,
        protocol_version: Option<ProtocolVersionId>,
        pubdata_limit: Option<u64>,
    ) -> UnsealedL1BatchHeader {
        UnsealedL1BatchHeader {
            number: self.number,
            timestamp: self.timestamp,
            protocol_version,
            fee_address: self.fee_account,
            fee_input: self.fee_input,
            pubdata_limit,
            settlement_layer: self.settlement_layer,
        }
    }
}
