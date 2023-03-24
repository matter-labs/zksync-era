use std::time::{Duration, Instant};

use vm::{
    vm_with_bootloader::{BlockContext, BlockContextMode},
    zk_evm::block_properties::BlockProperties,
};
use zksync_contracts::BaseSystemContracts;
use zksync_types::{Address, L1BatchNumber, U256, ZKPORTER_IS_AVAILABLE};
use zksync_utils::h256_to_u256;

use super::L1BatchParams;

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
