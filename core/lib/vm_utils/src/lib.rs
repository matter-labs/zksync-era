pub mod storage;

use anyhow::{anyhow, Context};
use multivm::{
    interface::{VmInterface, VmInterfaceHistoryEnabled},
    vm_latest::HistoryEnabled,
    HistoryMode, VmInstance,
};
use tokio::runtime::Handle;
use zksync_dal::StorageProcessor;
use zksync_state::{PostgresStorage, StoragePtr, StorageView, WriteStorage};
use zksync_types::{L1BatchNumber, L2ChainId, MiniblockNumber, Transaction};

use crate::storage::load_l1_batch_params;

pub type VmAndStorage<'a, H> = (
    VmInstance<StorageView<PostgresStorage<'a>>, H>,
    StoragePtr<StorageView<PostgresStorage<'a>>>,
);

pub async fn create_vm<H: HistoryMode>(
    l1_batch_number: L1BatchNumber,
    miniblock_number: Option<MiniblockNumber>,
    l2_chain_id: L2ChainId,
    mut pg_storage: PostgresStorage<'_>,
) -> anyhow::Result<VmAndStorage<H>> {
    let connection = pg_storage.connection();
    let fee_account_addr = connection
        .blocks_dal()
        .get_fee_address_for_l1_batch(l1_batch_number)
        .await?
        .with_context(|| {
            format!("l1_batch_number {l1_batch_number:?} must have fee_address_account")
        })?;

    let Some((prev_l1_batch_hash, _)) = connection
        .blocks_dal()
        .get_l1_batch_state_root_and_timestamp(l1_batch_number - 1)
        .await
        .unwrap()
    else {
        anyhow::bail!("l1_batch_number {l1_batch_number:?} must exists")
    };

    // In the state keeper, this value is used to reject execution.
    // All batches ran by BasicWitnessInputProducer have already been executed by State Keeper.
    // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
    let validation_computational_gas_limit = u32::MAX;
    let (system_env, l1_batch_env) = load_l1_batch_params(
        connection,
        l1_batch_number,
        fee_account_addr,
        validation_computational_gas_limit,
        l2_chain_id,
        miniblock_number,
        prev_l1_batch_hash,
    )
    .await
    .context("expected miniblock to be executed and sealed")?;

    let storage_view = StorageView::new(pg_storage).to_rc_ptr();
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

pub async fn create_vm_for_l1_batch<H: HistoryMode>(
    l1_batch_number: L1BatchNumber,
    l2_chain_id: L2ChainId,
    rt_handle: Handle,
    mut connection: StorageProcessor<'_>,
) -> anyhow::Result<VmAndStorage<H>> {
    let prev_l1_batch_number = l1_batch_number - 1;
    let (_, miniblock_number) = connection
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(prev_l1_batch_number)
        .await?
        .with_context(|| {
            format!(
                "l1_batch_number {l1_batch_number:?} must have a previous miniblock to start from"
            )
        })?;

    let pg_storage = PostgresStorage::new(rt_handle.clone(), connection, miniblock_number, true);

    create_vm::<H>(
        l1_batch_number,
        Some(miniblock_number + 1),
        l2_chain_id,
        pg_storage,
    )
    .await
}
