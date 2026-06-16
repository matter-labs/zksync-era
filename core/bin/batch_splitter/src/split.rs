//! Orchestration: load batch N, split its blocks in half, re-execute and
//! regenerate witnesses for each half, and assemble two
//! `AirbenderVerifierInput`s. Loading mirrors
//! `airbender_request_processor::airbender_verifier_input_for_existing_batch`.
//!
//! Order matters: piece B's execution depends on `root_A` (its
//! `previous_batch_hash`), and `root_A` comes from applying piece A to the
//! reconstructed sparse tree. So the flow is: re-exec A → sparse.process A →
//! root_A → re-exec B → sparse.process B (which feeds the `root_B ==
//! new_root_N` self-check).

use std::collections::HashMap;

use anyhow::Context;
use zksync_airbender_prover_interface::inputs::AirbenderVerifierInput;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_multivm::interface::{FinishedL1Batch, L2BlockEnv};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{
    commitment::L1BatchCommitmentMode, u256_to_h256, L1BatchNumber, L2ChainId, H256,
};
use zksync_vm_executor::storage::{L1BatchParamsProvider, RestoredL1BatchEnv};

use crate::{commitment, reexec, sparse_tree::SparseMerkle};

pub struct SplitParams {
    pub batch: L1BatchNumber,
    pub l2_chain_id: L2ChainId,
    pub commitment_chaining: bool,
}

pub async fn split(
    pool: &ConnectionPool<Core>,
    blob_store: &dyn ObjectStore,
    params: SplitParams,
) -> anyhow::Result<(AirbenderVerifierInput, AirbenderVerifierInput)> {
    let SplitParams {
        batch,
        l2_chain_id,
        commitment_chaining,
    } = params;

    let mut connection = pool.connection_tagged("batch_splitter").await?;

    // --- 1. Load the full batch's witness components (as the prover would) ---
    let vm_run_data: VMRunWitnessInputData = blob_store
        .get(batch)
        .await
        .context("failed to load VMRunWitnessInputData")?;
    let merkle_paths: WitnessInputMerklePaths = blob_store
        .get(batch)
        .await
        .context("failed to load WitnessInputMerklePaths")?;
    let start_enumeration_index = merkle_paths.next_enumeration_index();

    let l2_blocks = connection
        .transactions_dal()
        .get_l2_blocks_to_execute_for_l1_batch(batch)
        .await?;
    anyhow::ensure!(
        l2_blocks.len() >= 2,
        "batch {batch} has {} L2 block(s); need at least 2 to split",
        l2_blocks.len()
    );

    let l1_batch_params_provider = L1BatchParamsProvider::new(&mut connection).await?;
    let RestoredL1BatchEnv {
        system_env,
        l1_batch_env,
        pubdata_params,
        ..
    } = l1_batch_params_provider
        .load_l1_batch_env(&mut connection, batch, u32::MAX, l2_chain_id)
        .await?
        .with_context(|| format!("envs missing for batch {batch}"))?;
    let version = system_env.version;
    let commitment_mode: L1BatchCommitmentMode = pubdata_params.pubdata_type().into();

    let expected_new_root: H256 = connection
        .blocks_dal()
        .get_l1_batch_tree_data(batch)
        .await?
        .with_context(|| format!("tree data (root hash) missing for batch {batch}"))?
        .hash;

    // --- 2. Split the block list in half (whole blocks; fictive block → B) ---
    let mid = l2_blocks.len().div_ceil(2);
    let blocks_a = l2_blocks[..mid].to_vec();
    let blocks_b = l2_blocks[mid..].to_vec();
    tracing::info!(
        "splitting batch {batch} ({} blocks) at index {mid}: A blocks {}..{}, B blocks {}..{}",
        l2_blocks.len(),
        blocks_a.first().map(|b| b.number.0).unwrap_or_default(),
        blocks_a.last().map(|b| b.number.0).unwrap_or_default(),
        blocks_b.first().map(|b| b.number.0).unwrap_or_default(),
        blocks_b.last().map(|b| b.number.0).unwrap_or_default(),
    );

    // --- 3. Shared factory deps + the batch-start storage snapshot ---
    let factory_deps: HashMap<H256, Vec<u8>> = vm_run_data
        .used_bytecodes
        .iter()
        .map(|(hash, code)| (u256_to_h256(*hash), code.clone().into_flattened()))
        .collect();
    let start_storage = batch_start_storage(&vm_run_data);

    // --- 4. Re-execute piece A from the batch start ---
    let exec_a = reexec::reexecute_piece(
        &vm_run_data,
        l1_batch_env.clone(),
        system_env.clone(),
        pubdata_params,
        blocks_a.clone(),
        start_storage.clone(),
        factory_deps.clone(),
    )
    .context("re-execution of piece A failed")?;
    let logs_a = exec_a
        .finished
        .final_execution_state
        .deduplicated_storage_logs
        .clone();

    // --- 5. Reconstruct the N-1 tree from N's multiproof; regenerate A's paths.
    //        Non-invasive: uses only the immutable witness blob, no tree DB. ---
    let old_root = l1_batch_env
        .previous_batch_hash
        .context("batch N has no previous_batch_hash")?;
    let mut sparse = SparseMerkle::build(&merkle_paths, old_root, start_enumeration_index)?;
    let (merkle_paths_a, root_a) = sparse.process_piece(&logs_a)?;

    // --- 6. Re-execute piece B from (start + A's writes) with prev = root_A ---
    let storage_b = overlay_writes(start_storage, &exec_a.finished);
    let mut l1_batch_env_b = l1_batch_env.clone();
    l1_batch_env_b.first_l2_block = L2BlockEnv::from_l2_block_data(&blocks_b[0]);
    l1_batch_env_b.previous_batch_hash = Some(root_a);

    let exec_b = reexec::reexecute_piece(
        &vm_run_data,
        l1_batch_env_b.clone(),
        system_env.clone(),
        pubdata_params,
        blocks_b.clone(),
        storage_b,
        factory_deps,
    )
    .context("re-execution of piece B failed")?;
    let logs_b = exec_b
        .finished
        .final_execution_state
        .deduplicated_storage_logs
        .clone();

    // --- 7. Regenerate B's paths; self-check root_B == new_root_N ---
    let (merkle_paths_b, root_b) = sparse.process_piece(&logs_b)?;
    anyhow::ensure!(
        root_b == expected_new_root,
        "self-check failed: replayed root_B {root_b:?} != batch {batch} committed root \
         {expected_new_root:?}; the sparse reconstruction or leaf-index assignment is wrong",
    );

    // --- 8. Commitment inputs ---
    let pubdata_a = exec_a
        .finished
        .pubdata_input
        .clone()
        .context("piece A produced no pubdata")?;
    let pubdata_b = exec_b
        .finished
        .pubdata_input
        .clone()
        .context("piece B produced no pubdata")?;

    let ci_a = commitment::commitment_input_for_a(
        &mut connection,
        batch,
        &pubdata_a,
        &version,
        commitment_mode,
    )
    .await?;
    let ci_b = commitment::commitment_input_for_b(
        commitment_chaining,
        &pubdata_b,
        &version,
        commitment_mode,
    )?;

    // --- 9. Assemble the two self-contained inputs ---
    let input_a = AirbenderVerifierInput {
        vm_run_data: exec_a.vm_run_data,
        merkle_paths: merkle_paths_a,
        l2_blocks_execution_data: blocks_a,
        l1_batch_env,
        system_env: system_env.clone(),
        pubdata_params,
        commitment_input: Some(ci_a),
    };
    let input_b = AirbenderVerifierInput {
        vm_run_data: exec_b.vm_run_data,
        merkle_paths: merkle_paths_b,
        l2_blocks_execution_data: blocks_b,
        l1_batch_env: l1_batch_env_b,
        system_env,
        pubdata_params,
        commitment_input: ci_b,
    };

    Ok((input_a, input_b))
}

/// Batch-start storage snapshot, exactly as `AirbenderVerifierInput::verify`
/// builds it: read slots with dummy non-zero enum indices, plus initial-write
/// slots as `None`.
fn batch_start_storage(vm_run_data: &VMRunWitnessInputData) -> HashMap<H256, Option<(H256, u64)>> {
    let reads = vm_run_data
        .witness_block_state
        .read_storage_key
        .iter()
        .enumerate()
        .map(|(i, (key, value))| (key.hashed_key(), Some((*value, i as u64 + 1))));
    let initial_writes = vm_run_data
        .witness_block_state
        .is_write_initial
        .iter()
        .filter_map(|(key, initial)| initial.then_some((key.hashed_key(), None)));
    reads.chain(initial_writes).collect()
}

/// Overlay piece A's writes onto the start snapshot so piece B sees post-A
/// state. A non-zero enum index marks the slot as non-initial for B.
fn overlay_writes(
    mut storage: HashMap<H256, Option<(H256, u64)>>,
    finished_a: &FinishedL1Batch,
) -> HashMap<H256, Option<(H256, u64)>> {
    for log in &finished_a.final_execution_state.deduplicated_storage_logs {
        if log.is_write() {
            storage.insert(log.key.hashed_key(), Some((log.value, 1)));
        }
    }
    storage
}
