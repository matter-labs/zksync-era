use std::cell::RefCell;
use std::rc::Rc;

use crate::state_keeper::io::common::load_l1_batch_params;

use multivm::interface::L2BlockEnv;
use multivm::vm_latest::HistoryEnabled;
use multivm::VmInstance;
use tokio::runtime::Handle;
use zksync_dal::StorageProcessor;
use zksync_state::{PostgresStorage, ReadStorage, StorageView};
use zksync_types::block::MiniblockExecutionData;
use zksync_types::{L1BatchNumber, L2ChainId, Transaction};

pub(super) fn create_vm(
    rt_handle: Handle,
    l1_batch_number: L1BatchNumber,
    mut connection: StorageProcessor<'_>,
    validation_computational_gas_limit: u32,
    l2_chain_id: L2ChainId,
) -> (
    VmInstance<PostgresStorage, HistoryEnabled>,
    Rc<RefCell<StorageView<PostgresStorage>>>,
) {
    let prev_l1_batch_number = l1_batch_number - 1;
    let (_, miniblock_number) = rt_handle
        .block_on(
            connection
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(prev_l1_batch_number),
        )
        .unwrap()
        .unwrap_or_else(|| {
            panic!(
                "l1_batch_number {:?} must have a previous miniblock to start from",
                l1_batch_number
            )
        });

    let fee_account_addr = rt_handle
        .block_on(
            connection
                .blocks_dal()
                .get_fee_address_for_l1_batch(l1_batch_number),
        )
        .unwrap()
        .unwrap_or_else(|| {
            panic!(
                "l1_batch_number {:?} must have fee_address_account",
                l1_batch_number
            )
        });
    let (system_env, l1_batch_env) = rt_handle
        .block_on(load_l1_batch_params(
            // rt_handle.clone(),
            &mut connection,
            l1_batch_number,
            fee_account_addr,
            validation_computational_gas_limit,
            l2_chain_id,
        ))
        .unwrap();

    let pg_storage = PostgresStorage::new(rt_handle.clone(), connection, miniblock_number, true);
    let storage_view = StorageView::new(pg_storage).to_rc_ptr();
    let vm = VmInstance::new(l1_batch_env, system_env, storage_view.clone());

    (vm, storage_view)
}

pub(super) fn execute_tx<S: ReadStorage>(tx: &Transaction, vm: &mut VmInstance<S, HistoryEnabled>) {
    // attempt to run with bytecode compression
    vm.make_snapshot();
    if vm
        .inspect_transaction_with_bytecode_compression(vec![], tx.clone(), true)
        .is_ok()
    {
        vm.pop_snapshot_no_rollback();
        return;
    }

    // attempt to run without bytecode compression, if it failed with compression
    vm.rollback_to_the_latest_snapshot();
    vm.inspect_transaction_with_bytecode_compression(vec![], tx.clone(), false)
        .expect("Compression can't fail if we don't apply it");
}

pub(super) fn start_next_miniblock<S: ReadStorage>(
    vm: &mut VmInstance<S, HistoryEnabled>,
    miniblock_execution_data: Option<&MiniblockExecutionData>,
) {
    if let Some(miniblock_data) = miniblock_execution_data {
        let miniblock_state = L2BlockEnv {
            number: miniblock_data.number.0,
            timestamp: miniblock_data.timestamp,
            prev_block_hash: miniblock_data.prev_block_hash,
            max_virtual_blocks_to_create: miniblock_data.virtual_blocks,
        };
        vm.start_new_l2_block(miniblock_state);
    }
}
