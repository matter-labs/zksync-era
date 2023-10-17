use std::cell::RefCell;
use std::rc::Rc;

use crate::state_keeper::io::common::load_l1_batch_params;

use multivm::VmInstance;
use vm::{HistoryEnabled, L2BlockEnv};
use zksync_config::constants::{
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
};
use zksync_dal::StorageProcessor;
use zksync_state::{PostgresStorage, PostgresStorageCaches, ReadStorage, StorageView};
use zksync_types::block::unpack_block_info;
use zksync_types::{
    AccountTreeId, L1BatchNumber, L2ChainId, MiniblockNumber, StorageKey, Transaction,
};
use zksync_utils::h256_to_u256;

use tokio::runtime::Handle;

pub(super) async fn create_vm<'a>(
    rt_handle: Handle,
    l1_batch_number: L1BatchNumber,
    mut connection: StorageProcessor<'a>,
    validation_computational_gas_limit: u32,
    l2_chain_id: L2ChainId,
) -> (
    VmInstance<PostgresStorage<'a>, HistoryEnabled>,
    Rc<RefCell<StorageView<PostgresStorage<'a>>>>,
) {
    let prev_l1_batch_number = L1BatchNumber(l1_batch_number.0 - 1);
    let (_, miniblock_number) = connection
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(prev_l1_batch_number)
        .await
        .unwrap()
        .unwrap_or_else(|| {
            panic!(
                "l1_batch_number {:?} must have a previous miniblock to start from",
                l1_batch_number
            )
        });

    let fee_account_addr = connection
        .blocks_dal()
        .get_fee_address_for_l1_batch(&l1_batch_number)
        .await
        .unwrap()
        .unwrap_or_else(|| {
            panic!(
                "l1_batch_number {:?} must have fee_address_account",
                l1_batch_number
            )
        });
    let (system_env, l1_batch_env) = load_l1_batch_params(
        &mut connection,
        l1_batch_number,
        fee_account_addr,
        validation_computational_gas_limit,
        l2_chain_id,
    )
    .await
    .unwrap();

    let pg_storage = PostgresStorage::new(rt_handle.clone(), connection, miniblock_number, true)
        .with_caches(PostgresStorageCaches::new(
            128 * 1_024 * 1_024, // 128MB -- picked as the default value used in EN
            128 * 1_024 * 1_024, // 128MB -- picked as the default value used in EN
        ));
    let storage_view = StorageView::new(pg_storage).to_rc_ptr();
    let vm = VmInstance::new(l1_batch_env, system_env, storage_view.clone());

    (vm, storage_view)
}

pub(super) async fn get_miniblock_transition_state(
    connection: &mut StorageProcessor<'_>,
    miniblock_number: MiniblockNumber,
) -> L2BlockEnv {
    let l2_block_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let l2_block_info = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(&l2_block_info_key, miniblock_number + 1)
        .await
        .unwrap();

    let (next_miniblock_number, next_miniblock_timestamp) =
        unpack_block_info(h256_to_u256(l2_block_info));

    let miniblock_hash = connection
        .blocks_web3_dal()
        .get_miniblock_hash(miniblock_number)
        .await
        .unwrap()
        .unwrap();

    let virtual_blocks = connection
        .blocks_dal()
        .get_virtual_blocks_for_miniblock(&(miniblock_number + 1))
        .await
        .unwrap()
        .unwrap();

    L2BlockEnv {
        number: next_miniblock_number as u32,
        timestamp: next_miniblock_timestamp,
        prev_block_hash: miniblock_hash,
        max_virtual_blocks_to_create: virtual_blocks,
    }
}

pub(super) fn execute_tx<S: ReadStorage>(tx: &Transaction, vm: &mut VmInstance<S, HistoryEnabled>) {
    // attempt to run without bytecode compression
    vm.make_snapshot();
    if vm
        .inspect_transaction_with_bytecode_compression(vec![], tx.clone(), true)
        .is_ok()
    {
        vm.pop_snapshot_no_rollback();
        return;
    }

    // attempt to run with bytecode compression
    vm.rollback_to_the_latest_snapshot();
    vm.inspect_transaction_with_bytecode_compression(vec![], tx.clone(), false)
        .expect("Compression can't fail if we don't apply it");
}
