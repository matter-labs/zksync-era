use anyhow::{anyhow, Context};

use crate::state_keeper::io::common::load_l1_batch_params;

use multivm::interface::{VmInterface, VmInterfaceHistoryEnabled};
use multivm::vm_latest::HistoryEnabled;
use multivm::VmInstance;
use tokio::runtime::Handle;
use zksync_dal::StorageProcessor;
use zksync_state::{PostgresStorage, StoragePtr, StorageView, WriteStorage};
use zksync_types::{L1BatchNumber, L2ChainId, Transaction};

pub(super) type VmAndStorage<'a> = (
    VmInstance<StorageView<PostgresStorage<'a>>, HistoryEnabled>,
    StoragePtr<StorageView<PostgresStorage<'a>>>,
);

pub(super) fn create_vm(
    rt_handle: Handle,
    l1_batch_number: L1BatchNumber,
    mut connection: StorageProcessor<'_>,
    l2_chain_id: L2ChainId,
) -> anyhow::Result<VmAndStorage> {
    let prev_l1_batch_number = l1_batch_number - 1;
    let (_, miniblock_number) = rt_handle
        .block_on(
            connection
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(prev_l1_batch_number),
        )?
        .with_context(|| {
            format!(
                "l1_batch_number {l1_batch_number:?} must have a previous miniblock to start from"
            )
        })?;

    let fee_account_addr = rt_handle
        .block_on(
            connection
                .blocks_dal()
                .get_fee_address_for_l1_batch(l1_batch_number),
        )?
        .with_context(|| {
            format!("l1_batch_number {l1_batch_number:?} must have fee_address_account")
        })?;

    // In the state keeper, this value is used to reject execution.
    // All batches ran by BasicWitnessInputProducer have already been executed by State Keeper.
    // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
    let validation_computational_gas_limit = u32::MAX;
    let (system_env, l1_batch_env) = rt_handle
        .block_on(load_l1_batch_params(
            &mut connection,
            l1_batch_number,
            fee_account_addr,
            validation_computational_gas_limit,
            l2_chain_id,
        ))
        .context("expected miniblock to be executed and sealed")?;

    let pg_storage = PostgresStorage::new(rt_handle.clone(), connection, miniblock_number, true);
    let storage_view = StorageView::new(pg_storage).to_rc_ptr();
    let vm = VmInstance::new(l1_batch_env, system_env, storage_view.clone());

    Ok((vm, storage_view))
}

pub(super) fn execute_tx<S: WriteStorage>(
    tx: &Transaction,
    vm: &mut VmInstance<S, HistoryEnabled>,
) -> anyhow::Result<()> {
    // Attempt to run VM with bytecode compression on.
    vm.make_snapshot();
    if vm
        .execute_transaction_with_bytecode_compression(tx.clone(), true)
        .is_ok()
    {
        vm.pop_snapshot_no_rollback();
        return Ok(());
    }

    // If failed with bytecode compression, attempt to run without bytecode compression.
    vm.rollback_to_the_latest_snapshot();
    if vm
        .execute_transaction_with_bytecode_compression(tx.clone(), false)
        .is_err()
    {
        return Err(anyhow!("compression can't fail if we don't apply it"));
    }
    Ok(())
}
