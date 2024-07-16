pub mod storage;

use anyhow::{anyhow, Context};
use tokio::runtime::Handle;
use zksync_dal::{Connection, Core};
use zksync_multivm::{
    interface::{VmInterface, VmInterfaceHistoryEnabled},
    vm_latest::HistoryEnabled,
    VmInstance,
};
use zksync_state::{PostgresStorage, StorageOverrides, StoragePtr, StorageView, WriteStorage};
use zksync_types::{L1BatchNumber, L2ChainId, Transaction};

use crate::storage::L1BatchParamsProvider;

pub type VmAndStorage<'a> = (
    VmInstance<StorageView<StorageOverrides<PostgresStorage<'a>>>, HistoryEnabled>,
    StoragePtr<StorageView<StorageOverrides<PostgresStorage<'a>>>>,
);

pub fn create_vm(
    rt_handle: Handle,
    l1_batch_number: L1BatchNumber,
    mut connection: Connection<'_, Core>,
    l2_chain_id: L2ChainId,
) -> anyhow::Result<VmAndStorage> {
    let mut l1_batch_params_provider = L1BatchParamsProvider::new();
    rt_handle
        .block_on(l1_batch_params_provider.initialize(&mut connection))
        .context("failed initializing L1 batch params provider")?;
    let first_l2_block_in_batch = rt_handle
        .block_on(
            l1_batch_params_provider.load_first_l2_block_in_batch(&mut connection, l1_batch_number),
        )
        .with_context(|| format!("failed loading first L2 block in L1 batch #{l1_batch_number}"))?
        .with_context(|| format!("no L2 blocks persisted for L1 batch #{l1_batch_number}"))?;

    // In the state keeper, this value is used to reject execution.
    // All batches ran by BasicWitnessInputProducer have already been executed by State Keeper.
    // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
    let validation_computational_gas_limit = u32::MAX;

    let (system_env, l1_batch_env) = rt_handle
        .block_on(l1_batch_params_provider.load_l1_batch_params(
            &mut connection,
            &first_l2_block_in_batch,
            validation_computational_gas_limit,
            l2_chain_id,
        ))
        .context("expected L2 block to be executed and sealed")?;

    let storage_l2_block_number = first_l2_block_in_batch.number() - 1;
    let pg_storage =
        PostgresStorage::new(rt_handle.clone(), connection, storage_l2_block_number, true);
    let storage_overrides = StorageOverrides::new(pg_storage);
    let storage_view = StorageView::new(storage_overrides).to_rc_ptr();
    let vm = VmInstance::new(l1_batch_env, system_env, storage_view.clone());

    Ok((vm, storage_view))
}

pub fn execute_tx<S: WriteStorage>(
    tx: &Transaction,
    vm: &mut VmInstance<S, HistoryEnabled>,
) -> anyhow::Result<()> {
    // Attempt to run VM with bytecode compression on.
    vm.make_snapshot();
    if vm
        .execute_transaction_with_bytecode_compression(tx.clone(), true)
        .0
        .is_ok()
    {
        vm.pop_snapshot_no_rollback();
        return Ok(());
    }

    // If failed with bytecode compression, attempt to run without bytecode compression.
    vm.rollback_to_the_latest_snapshot();
    if vm
        .execute_transaction_with_bytecode_compression(tx.clone(), false)
        .0
        .is_err()
    {
        return Err(anyhow!("compression can't fail if we don't apply it"));
    }
    Ok(())
}
