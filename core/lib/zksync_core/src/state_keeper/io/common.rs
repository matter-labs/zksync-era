use std::time::Duration;

use multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BLOCK_GAS_LIMIT,
};
use vm_utils::storage::load_l1_batch_params;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_types::{
    fee_model::BatchFeeInput, Address, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, H256, U256, ZKPORTER_IS_AVAILABLE,
};
use zksync_utils::u256_to_h256;

use super::PendingBatchData;

/// Returns the parameters required to initialize the VM for the next L1 batch.
#[allow(clippy::too_many_arguments)]
pub(crate) fn l1_batch_params(
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    l1_batch_timestamp: u64,
    previous_batch_hash: U256,
    fee_input: BatchFeeInput,
    first_miniblock_number: MiniblockNumber,
    prev_miniblock_hash: H256,
    base_system_contracts: BaseSystemContracts,
    validation_computational_gas_limit: u32,
    protocol_version: ProtocolVersionId,
    virtual_blocks: u32,
    chain_id: L2ChainId,
) -> (SystemEnv, L1BatchEnv) {
    (
        SystemEnv {
            zk_porter_available: ZKPORTER_IS_AVAILABLE,
            version: protocol_version,
            base_system_smart_contracts: base_system_contracts,
            gas_limit: BLOCK_GAS_LIMIT,
            execution_mode: TxExecutionMode::VerifyExecute,
            default_validation_computational_gas_limit: validation_computational_gas_limit,
            chain_id,
        },
        L1BatchEnv {
            previous_batch_hash: Some(u256_to_h256(previous_batch_hash)),
            number: current_l1_batch_number,
            timestamp: l1_batch_timestamp,
            fee_input,
            fee_account,
            enforced_base_fee: None,
            first_l2_block: L2BlockEnv {
                number: first_miniblock_number.0,
                timestamp: l1_batch_timestamp,
                prev_block_hash: prev_miniblock_hash,
                max_virtual_blocks_to_create: virtual_blocks,
            },
        },
    )
}

/// Returns the amount of iterations `delay_interval` fits into `max_wait`, rounding up.
pub(crate) fn poll_iters(delay_interval: Duration, max_wait: Duration) -> usize {
    let max_wait_millis = max_wait.as_millis() as u64;
    let delay_interval_millis = delay_interval.as_millis() as u64;
    assert!(delay_interval_millis > 0, "delay interval must be positive");

    ((max_wait_millis + delay_interval_millis - 1) / delay_interval_millis).max(1) as usize
}

/// Loads the pending L1 block data from the database.
pub(crate) async fn load_pending_batch(
    storage: &mut StorageProcessor<'_>,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    validation_computational_gas_limit: u32,
    chain_id: L2ChainId,
) -> Option<PendingBatchData> {
    let (system_env, l1_batch_env) = load_l1_batch_params(
        storage,
        current_l1_batch_number,
        fee_account,
        validation_computational_gas_limit,
        chain_id,
    )
    .await?;

    let pending_miniblocks = storage
        .transactions_dal()
        .get_miniblocks_to_reexecute()
        .await
        .unwrap();

    Some(PendingBatchData {
        l1_batch_env,
        system_env,
        pending_miniblocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip] // One-line formatting looks better here.
    fn test_poll_iters() {
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(0)), 1);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(100)), 1);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(101)), 2);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(200)), 2);
        assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(201)), 3);
    }
}
