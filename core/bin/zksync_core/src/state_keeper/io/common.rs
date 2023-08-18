use std::time::Duration;

use vm::{
    vm_with_bootloader::{BlockContext, BlockContextMode},
    zk_evm::block_properties::BlockProperties,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_types::{Address, L1BatchNumber, ProtocolVersionId, U256, ZKPORTER_IS_AVAILABLE};
use zksync_utils::h256_to_u256;

use itertools::Itertools;

use super::{L1BatchParams, PendingBatchData};
use crate::state_keeper::extractors;

/// Returns the parameters required to initialize the VM for the next L1 batch.
#[allow(clippy::too_many_arguments)]
pub(crate) fn l1_batch_params(
    current_l1_batch_number: L1BatchNumber,
    operator_address: Address,
    l1_batch_timestamp: u64,
    previous_block_hash: U256,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    base_system_contracts: BaseSystemContracts,
    protocol_version: ProtocolVersionId,
) -> L1BatchParams {
    let block_properties = BlockProperties {
        default_aa_code_hash: h256_to_u256(base_system_contracts.default_aa.hash),
        zkporter_is_available: ZKPORTER_IS_AVAILABLE,
    };

    let context = BlockContext {
        block_number: current_l1_batch_number.0,
        block_timestamp: l1_batch_timestamp,
        l1_gas_price,
        fair_l2_gas_price,
        operator_address,
    };

    L1BatchParams {
        context_mode: BlockContextMode::NewBlock(context.into(), previous_block_hash),
        properties: block_properties,
        base_system_contracts,
        protocol_version,
    }
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
) -> Option<PendingBatchData> {
    // If pending miniblock doesn't exist, it means that there is no unsynced state (i.e. no transaction
    // were executed after the last sealed batch).
    let pending_miniblock_number = {
        let (_, last_miniblock_number_included_in_l1_batch) = storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(current_l1_batch_number - 1)
            .await
            .unwrap();
        last_miniblock_number_included_in_l1_batch + 1
    };
    let pending_miniblock_header = storage
        .blocks_dal()
        .get_miniblock_header(pending_miniblock_number)
        .await?;

    vlog::info!("Getting previous batch hash");
    let (previous_l1_batch_hash, _) =
        extractors::wait_for_prev_l1_batch_params(storage, current_l1_batch_number).await;

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

    vlog::info!("Previous l1_batch_hash: {}", previous_l1_batch_hash);
    let params = l1_batch_params(
        current_l1_batch_number,
        fee_account,
        pending_miniblock_header.timestamp,
        previous_l1_batch_hash,
        pending_miniblock_header.l1_gas_price,
        pending_miniblock_header.l2_fair_gas_price,
        base_system_contracts,
        pending_miniblock_header
            .protocol_version
            .expect("`protocol_version` must be set for pending miniblock"),
    );

    let pending_miniblocks = storage
        .transactions_dal()
        .get_miniblocks_to_reexecute()
        .await;

    Some(PendingBatchData {
        params,
        pending_miniblocks,
    })
}

/// Sets missing initial writes indices.
pub async fn set_missing_initial_writes_indices(storage: &mut StorageProcessor<'_>) {
    // Indices should start from 1, that's why default is (1, 0).
    let (mut next_index, start_from_batch) = storage
        .storage_logs_dedup_dal()
        .max_set_enumeration_index()
        .await
        .map(|(index, l1_batch_number)| (index + 1, l1_batch_number + 1))
        .unwrap_or((1, L1BatchNumber(0)));

    let sealed_batch = storage.blocks_dal().get_sealed_l1_batch_number().await;
    if start_from_batch > sealed_batch {
        vlog::info!("All indices for initial writes are already set, no action is needed");
        return;
    } else {
        let batches_count = sealed_batch.0 - start_from_batch.0 + 1;
        if batches_count > 100 {
            vlog::warn!("There are {batches_count} batches to set indices for, it may take substantial time.");
        }
    }

    vlog::info!(
        "Last set index {}. Starting migration from batch {start_from_batch}",
        next_index - 1
    );
    let mut current_l1_batch = start_from_batch;
    loop {
        if current_l1_batch > storage.blocks_dal().get_sealed_l1_batch_number().await {
            break;
        }
        vlog::info!("Setting indices for batch {current_l1_batch}");

        let (hashed_keys, _): (Vec<_>, Vec<_>) = storage
            .storage_logs_dedup_dal()
            .initial_writes_for_batch(current_l1_batch)
            .await
            .into_iter()
            .unzip();
        let storage_keys = storage
            .storage_logs_dal()
            .resolve_hashed_keys(&hashed_keys)
            .await;

        // Sort storage key alphanumerically and assign indices.
        let indexed_keys: Vec<_> = storage_keys
            .into_iter()
            .sorted()
            .enumerate()
            .map(|(pos, key)| (key.hashed_key(), next_index + pos as u64))
            .collect();
        storage
            .storage_logs_dedup_dal()
            .set_indices_for_initial_writes(&indexed_keys)
            .await;

        next_index += indexed_keys.len() as u64;
        current_l1_batch += 1;
    }
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
