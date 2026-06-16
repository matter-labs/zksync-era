//! Re-execution of one block range of a sealed batch.
//!
//! Mirrors `core/lib/airbender_verifier/src/lib.rs` (`verify` / `execute_vm`):
//! a `StorageSnapshot` seeded from the witness is enough to drive the
//! bootloader over a list of L2 blocks. We reuse the *full* batch's
//! `VMRunWitnessInputData` as a template for the fields that are identical
//! across pieces (system contracts, the superset of used bytecodes) and
//! override only the five fields that are genuinely per-piece.

use std::collections::HashMap;

use anyhow::Context;
use zksync_multivm::{
    interface::{
        storage::{ReadStorage, StorageSnapshot, StorageView},
        FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmFactory, VmInterface, VmInterfaceExt,
        VmInterfaceHistoryEnabled,
    },
    pubdata_builders::pubdata_params_to_builder,
    vm_latest::HistoryEnabled,
    LegacyVmInstance,
};
use zksync_prover_interface::inputs::VMRunWitnessInputData;
use zksync_types::{
    block::L2BlockExecutionData, commitment::PubdataParams,
    witness_block_state::WitnessStorageState, ProtocolVersionId, Transaction, H256,
};

/// Result of re-executing one piece.
pub struct PieceExecution {
    /// Per-piece witness data ready to drop into an `AirbenderVerifierInput`.
    pub vm_run_data: VMRunWitnessInputData,
    /// Raw VM output; the caller needs `final_execution_state` for tree
    /// instructions (piece A) and storage chaining (piece B → A's writes).
    pub finished: FinishedL1Batch,
}

/// Re-execute `blocks` against `storage` (+ `factory_deps`), producing the
/// piece's witness data. `template` is the full batch's `VMRunWitnessInputData`,
/// reused for the contract/bytecode fields that don't change between pieces.
pub fn reexecute_piece(
    template: &VMRunWitnessInputData,
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
    pubdata_params: PubdataParams,
    blocks: Vec<L2BlockExecutionData>,
    storage: HashMap<H256, Option<(H256, u64)>>,
    factory_deps: HashMap<H256, Vec<u8>>,
) -> anyhow::Result<PieceExecution> {
    anyhow::ensure!(!blocks.is_empty(), "cannot re-execute an empty block range");

    let l1_batch_number = l1_batch_env.number;
    let version = system_env.version;

    let storage_snapshot = StorageSnapshot::new(storage, factory_deps);
    let storage_view = StorageView::new(storage_snapshot).to_rc_ptr();
    let vm = LegacyVmInstance::new(l1_batch_env, system_env, storage_view.clone());

    let finished = execute_vm(blocks, vm, pubdata_params, version)?;

    // Read keys + initial-write flags actually observed by *this* piece.
    let cache = storage_view.borrow().cache();
    let witness_block_state = WitnessStorageState {
        read_storage_key: cache.read_storage_keys(),
        is_write_initial: cache.initial_writes(),
    };

    let initial_heap_content = finished
        .final_bootloader_memory
        .clone()
        .context("final bootloader memory missing after re-execution")?;

    let vm_run_data = VMRunWitnessInputData {
        l1_batch_number,
        // Superset of bytecodes; safe for proving a subrange. Could be pruned
        // to exactly the bytecodes this piece touched as a later refinement.
        used_bytecodes: template.used_bytecodes.clone(),
        initial_heap_content,
        protocol_version: version,
        bootloader_code: template.bootloader_code.clone(),
        default_account_code_hash: template.default_account_code_hash,
        evm_emulator_code_hash: template.evm_emulator_code_hash,
        storage_refunds: finished.final_execution_state.storage_refunds.clone(),
        pubdata_costs: finished.final_execution_state.pubdata_costs.clone(),
        witness_block_state,
    };

    Ok(PieceExecution {
        vm_run_data,
        finished,
    })
}

// --- replicated from airbender_verifier (private there) ---

fn execute_vm<S: ReadStorage>(
    l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    mut vm: LegacyVmInstance<S, HistoryEnabled>,
    pubdata_params: PubdataParams,
    protocol_version: ProtocolVersionId,
) -> anyhow::Result<FinishedL1Batch> {
    let next_l2_blocks_data = l2_blocks_execution_data.iter().skip(1);
    let l2_blocks_data = l2_blocks_execution_data.iter().zip(next_l2_blocks_data);

    for (l2_block_data, next_l2_block_data) in l2_blocks_data {
        for tx in &l2_block_data.txs {
            execute_tx(tx, &mut vm).context("failed to execute transaction in batch_splitter")?;
        }
        vm.start_new_l2_block(L2BlockEnv::from_l2_block_data(next_l2_block_data));
    }

    Ok(vm.finish_batch(pubdata_params_to_builder(pubdata_params, protocol_version)))
}

fn execute_tx<S: ReadStorage>(
    tx: &Transaction,
    vm: &mut LegacyVmInstance<S, HistoryEnabled>,
) -> anyhow::Result<()> {
    vm.make_snapshot();
    if vm
        .execute_transaction_with_bytecode_compression(tx.clone(), true)
        .0
        .is_ok()
    {
        vm.pop_snapshot_no_rollback();
        return Ok(());
    }
    vm.rollback_to_the_latest_snapshot();
    if vm
        .execute_transaction_with_bytecode_compression(tx.clone(), false)
        .0
        .is_err()
    {
        anyhow::bail!("compression can't fail if we don't apply it");
    }
    Ok(())
}
