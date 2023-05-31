use std::time::{Duration, Instant};

use vm::{
    vm_with_bootloader::{BlockContext, BlockContextMode},
    zk_evm::block_properties::BlockProperties,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_types::{Address, L1BatchNumber, U256, ZKPORTER_IS_AVAILABLE};
use zksync_utils::h256_to_u256;

use crate::state_keeper::extractors;

use super::{L1BatchParams, PendingBatchData};

#[derive(Debug)]
pub(crate) struct StateKeeperStats {
    pub(crate) num_contracts: u64,
}

/// Returns the parameters required to initialize the VM for the next L1 batch.
pub(crate) fn l1_batch_params(
    current_l1_batch_number: L1BatchNumber,
    operator_address: Address,
    l1_batch_timestamp: u64,
    previous_block_hash: U256,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    base_system_contracts: BaseSystemContracts,
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
    }
}

/// Runs the provided closure `f` until it returns `Some` or the `max_wait` time has elapsed.
pub(crate) fn poll_until<T, F: FnMut() -> Option<T>>(
    delay_interval: Duration,
    max_wait: Duration,
    mut f: F,
) -> Option<T> {
    let wait_interval = delay_interval.min(max_wait);
    let start = Instant::now();
    while start.elapsed() <= max_wait {
        let res = f();
        if res.is_some() {
            return res;
        }
        std::thread::sleep(wait_interval);
    }
    None
}

/// Loads the pending L1 block data from the database.
pub(crate) fn load_pending_batch(
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
            .unwrap();
        last_miniblock_number_included_in_l1_batch + 1
    };
    let pending_miniblock_header = storage
        .blocks_dal()
        .get_miniblock_header(pending_miniblock_number)?;

    vlog::info!("Getting previous batch hash");
    let previous_l1_batch_hash =
        extractors::wait_for_prev_l1_batch_state_root_unchecked(storage, current_l1_batch_number);

    let base_system_contracts = storage.storage_dal().get_base_system_contracts(
        pending_miniblock_header
            .base_system_contracts_hashes
            .bootloader,
        pending_miniblock_header
            .base_system_contracts_hashes
            .default_aa,
    );

    vlog::info!("Previous l1_batch_hash: {}", previous_l1_batch_hash);
    let params = l1_batch_params(
        current_l1_batch_number,
        fee_account,
        pending_miniblock_header.timestamp,
        previous_l1_batch_hash,
        pending_miniblock_header.l1_gas_price,
        pending_miniblock_header.l2_fair_gas_price,
        base_system_contracts,
    );

    let txs = storage.transactions_dal().get_transactions_to_reexecute();

    Some(PendingBatchData { params, txs })
}
