use std::time::{Duration, Instant};

use multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BLOCK_GAS_LIMIT,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_types::{
    fee_model::BatchFeeInput, Address, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, H256, U256, ZKPORTER_IS_AVAILABLE,
};
use zksync_utils::{h256_to_u256, u256_to_h256};

pub async fn load_l1_batch_params(
    storage: &mut StorageProcessor<'_>,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    validation_computational_gas_limit: u32,
    chain_id: L2ChainId,
) -> Option<(SystemEnv, L1BatchEnv)> {
    // If miniblock doesn't exist (for instance if it's pending), it means that there is no unsynced state (i.e. no transactions
    // were executed after the last sealed batch).
    let pending_miniblock_number = {
        let (_, last_miniblock_number_included_in_l1_batch) = storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(current_l1_batch_number - 1)
            .await
            .unwrap()
            .unwrap();
        last_miniblock_number_included_in_l1_batch + 1
    };
    let pending_miniblock_header = storage
        .blocks_dal()
        .get_miniblock_header(pending_miniblock_number)
        .await
        .unwrap()?;

    tracing::info!("Getting previous batch hash");
    let (previous_l1_batch_hash, _) =
        wait_for_prev_l1_batch_params(storage, current_l1_batch_number).await;

    tracing::info!("Getting previous miniblock hash");
    let prev_miniblock_hash = storage
        .blocks_dal()
        .get_miniblock_header(pending_miniblock_number - 1)
        .await
        .unwrap()
        .unwrap()
        .hash;

    let base_system_contracts = storage
        .storage_dal()
        .get_base_system_contracts(
            pending_miniblock_header
                .base_system_contracts_hashes
                .bootloader,
            pending_miniblock_header
                .base_system_contracts_hashes
                .default_aa,
        )
        .await;

    tracing::info!("Previous l1_batch_hash: {}", previous_l1_batch_hash);
    Some(l1_batch_params(
        current_l1_batch_number,
        fee_account,
        pending_miniblock_header.timestamp,
        previous_l1_batch_hash,
        pending_miniblock_header.batch_fee_input,
        pending_miniblock_number,
        prev_miniblock_hash,
        base_system_contracts,
        validation_computational_gas_limit,
        pending_miniblock_header
            .protocol_version
            .expect("`protocol_version` must be set for pending miniblock"),
        pending_miniblock_header.virtual_blocks,
        chain_id,
    ))
}

pub async fn wait_for_prev_l1_batch_params(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> (U256, u64) {
    if number == L1BatchNumber(0) {
        return (U256::default(), 0);
    }
    wait_for_l1_batch_params_unchecked(storage, number - 1).await
}

/// # Warning
///
/// If invoked for a `L1BatchNumber` of a non-existent l1 batch, will block current thread indefinitely.
async fn wait_for_l1_batch_params_unchecked(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> (U256, u64) {
    // If the state root is not known yet, this duration will be used to back off in the while loops
    const SAFE_STATE_ROOT_INTERVAL: Duration = Duration::from_millis(100);

    let stage_started_at: Instant = Instant::now();
    loop {
        let data = storage
            .blocks_dal()
            .get_l1_batch_state_root_and_timestamp(number)
            .await
            .unwrap();
        if let Some((root_hash, timestamp)) = data {
            tracing::trace!(
                "Waiting for hash of L1 batch #{number} took {:?}",
                stage_started_at.elapsed()
            );
            return (h256_to_u256(root_hash), timestamp);
        }

        tokio::time::sleep(SAFE_STATE_ROOT_INTERVAL).await;
    }
}

/// Returns the parameters required to initialize the VM for the next L1 batch.
#[allow(clippy::too_many_arguments)]
pub fn l1_batch_params(
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
