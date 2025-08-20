//! Tee verifier
//!
//! Verifies that a L1Batch has the expected root hash after
//! executing the VM and verifying all the accessed memory slots by their
//! merkle path.

use anyhow::{bail, Context, Result};
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    BlockOutputWithProofs, TreeInstruction, TreeLogEntry, TreeLogEntryWithProof, ValueHash,
};
use zksync_multivm::{
    interface::{
        storage::{ReadStorage, StorageSnapshot, StorageView},
        FinishedL1Batch, L2BlockEnv, VmFactory, VmInterface, VmInterfaceExt,
        VmInterfaceHistoryEnabled,
    },
    pubdata_builders::pubdata_params_to_builder,
    vm_latest::HistoryEnabled,
    LegacyVmInstance,
};
use zksync_prover_interface::inputs::{StorageLogMetadata, WitnessInputMerklePaths};
use zksync_tee_prover_interface::inputs::V1TeeVerifierInput;
use zksync_types::{
    block::L2BlockExecutionData, commitment::PubdataParams, u256_to_h256, L1BatchNumber,
    ProtocolVersionId, StorageLog, StorageValue, Transaction, H256,
};

/// A structure to hold the result of verification.
pub struct VerificationResult {
    /// The root hash of the batch that was verified.
    pub value_hash: ValueHash,
    /// The batch number that was verified.
    pub batch_number: L1BatchNumber,
}

/// A trait for the computations that can be verified in TEE.
pub trait Verify {
    fn verify(self) -> anyhow::Result<VerificationResult>;
}

impl Verify for V1TeeVerifierInput {
    /// Verify that the L1Batch produces the expected root hash
    /// by executing the VM and verifying the merkle paths of all
    /// touch storage slots.
    ///
    /// # Errors
    ///
    /// Returns a verbose error of the failure, because any error is
    /// not actionable.
    fn verify(self) -> anyhow::Result<VerificationResult> {
        let old_root_hash = self.l1_batch_env.previous_batch_hash.unwrap();
        let enumeration_index = self.merkle_paths.next_enumeration_index();
        let batch_number = self.l1_batch_env.number;

        let read_storage_ops = self
            .vm_run_data
            .witness_block_state
            .read_storage_key
            .into_iter();

        let initial_writes_ops = self
            .vm_run_data
            .witness_block_state
            .is_write_initial
            .into_iter();

        // We need to define storage slots read during batch execution, and their initial state;
        // hence, the use of both read_storage_ops and initial_writes_ops.
        // StorageSnapshot also requires providing enumeration indices,
        // but they only matter at the end of execution when creating pubdata for the batch,
        // which is irrelevant in this case. Thus, enumeration indices are set to dummy values.
        let storage = read_storage_ops
            .enumerate()
            .map(|(i, (hash, bytes))| (hash.hashed_key(), Some((bytes, i as u64 + 1u64))))
            .chain(initial_writes_ops.filter_map(|(key, initial_write)| {
                initial_write.then_some((key.hashed_key(), None))
            }))
            .collect();

        let factory_deps = self
            .vm_run_data
            .used_bytecodes
            .into_iter()
            .map(|(hash, bytes)| (u256_to_h256(hash), bytes.into_flattened()))
            .collect();

        let storage_snapshot = StorageSnapshot::new(storage, factory_deps);
        let storage_view = StorageView::new(storage_snapshot).to_rc_ptr();
        let vm = LegacyVmInstance::new(self.l1_batch_env, self.system_env.clone(), storage_view);
        let vm_out = execute_vm(
            self.l2_blocks_execution_data,
            vm,
            self.pubdata_params,
            self.system_env.version,
        )?;

        let block_output_with_proofs = get_bowp(self.merkle_paths)?;

        let instructions: Vec<TreeInstruction> =
            generate_tree_instructions(enumeration_index, &block_output_with_proofs, vm_out)?;

        block_output_with_proofs
            .verify_proofs(&Blake2Hasher, old_root_hash, &instructions)
            .context("Failed to verify_proofs {l1_batch_number} correctly!")?;

        Ok(VerificationResult {
            value_hash: block_output_with_proofs.root_hash().unwrap(),
            batch_number,
        })
    }
}

/// Sets the initial storage values and returns `BlockOutputWithProofs`
fn get_bowp(witness_input_merkle_paths: WitnessInputMerklePaths) -> Result<BlockOutputWithProofs> {
    let logs_result: Result<_, _> = witness_input_merkle_paths
        .into_merkle_paths()
        .map(
            |StorageLogMetadata {
                 root_hash,
                 merkle_paths,
                 is_write,
                 first_write,
                 leaf_enumeration_index,
                 value_read,
                 leaf_hashed_key: leaf_storage_key,
                 ..
             }| {
                let root_hash = root_hash.into();
                let merkle_path = merkle_paths.into_iter().map(|x| x.into()).collect();
                let base: TreeLogEntry = match (is_write, first_write, leaf_enumeration_index) {
                    (false, _, 0) => TreeLogEntry::ReadMissingKey,
                    (false, false, _) => {
                        // This is a special U256 here, which needs `to_little_endian`
                        let mut hashed_key = [0_u8; 32];
                        leaf_storage_key.to_little_endian(&mut hashed_key);
                        tracing::trace!(
                            "TreeLogEntry::Read {leaf_storage_key:x} = {:x}",
                            StorageValue::from(value_read)
                        );
                        TreeLogEntry::Read {
                            leaf_index: leaf_enumeration_index,
                            value: value_read.into(),
                        }
                    }
                    (false, true, _) => {
                        tracing::error!("get_bowp is_write = false, first_write = true");
                        bail!("get_bowp is_write = false, first_write = true");
                    }
                    (true, true, _) => TreeLogEntry::Inserted,
                    (true, false, _) => {
                        // This is a special U256 here, which needs `to_little_endian`
                        let mut hashed_key = [0_u8; 32];
                        leaf_storage_key.to_little_endian(&mut hashed_key);
                        tracing::trace!(
                            "TreeLogEntry::Updated {leaf_storage_key:x} = {:x}",
                            StorageValue::from(value_read)
                        );
                        TreeLogEntry::Updated {
                            leaf_index: leaf_enumeration_index,
                            previous_value: value_read.into(),
                        }
                    }
                };
                Ok(TreeLogEntryWithProof {
                    base,
                    merkle_path,
                    root_hash,
                })
            },
        )
        .collect();

    let logs: Vec<TreeLogEntryWithProof> = logs_result?;

    Ok(BlockOutputWithProofs {
        logs,
        leaf_count: 0,
    })
}

/// Executes the VM and returns `FinishedL1Batch` on success.
fn execute_vm<S: ReadStorage>(
    l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    mut vm: LegacyVmInstance<S, HistoryEnabled>,
    pubdata_params: PubdataParams,
    protocol_version: ProtocolVersionId,
) -> anyhow::Result<FinishedL1Batch> {
    let next_l2_blocks_data = l2_blocks_execution_data.iter().skip(1);

    let l2_blocks_data = l2_blocks_execution_data.iter().zip(next_l2_blocks_data);

    for (l2_block_data, next_l2_block_data) in l2_blocks_data {
        tracing::trace!(
            "Started execution of l2_block: {:?}, executing {:?} transactions",
            l2_block_data.number,
            l2_block_data.txs.len(),
        );
        for tx in &l2_block_data.txs {
            tracing::trace!("Started execution of tx: {tx:?}");
            execute_tx(tx, &mut vm)
                .context("failed to execute transaction in TeeVerifierInputProducer")?;
            tracing::trace!("Finished execution of tx: {tx:?}");
        }

        tracing::trace!("finished l2_block {l2_block_data:?}");
        tracing::trace!("about to vm.start_new_l2_block {next_l2_block_data:?}");

        vm.start_new_l2_block(L2BlockEnv::from_l2_block_data(next_l2_block_data));

        tracing::trace!("Finished execution of l2_block: {:?}", l2_block_data.number);
    }

    tracing::trace!("about to vm.finish_batch()");

    Ok(vm.finish_batch(pubdata_params_to_builder(pubdata_params, protocol_version)))
}

/// Map `LogQuery` and `TreeLogEntry` to a `TreeInstruction`
fn map_log_tree(
    storage_log: &StorageLog,
    tree_log_entry: &TreeLogEntry,
    idx: &mut u64,
) -> anyhow::Result<TreeInstruction> {
    let key = storage_log.key.hashed_key_u256();
    let tree_instruction = match (storage_log.is_write(), *tree_log_entry) {
        (true, TreeLogEntry::Updated { leaf_index, .. }) => {
            TreeInstruction::write(key, leaf_index, H256(storage_log.value.into()))
        }
        (true, TreeLogEntry::Inserted) => {
            let leaf_index = *idx;
            *idx += 1;
            TreeInstruction::write(key, leaf_index, H256(storage_log.value.into()))
        }
        (false, TreeLogEntry::Read { value, .. }) => {
            if storage_log.value != value {
                tracing::error!(
                    ?storage_log,
                    ?tree_log_entry,
                    "Failed to map LogQuery to TreeInstruction: read value {:#?} != {:#?}",
                    storage_log.value,
                    value
                );
                anyhow::bail!("Failed to map LogQuery to TreeInstruction");
            }
            TreeInstruction::Read(key)
        }
        (false, TreeLogEntry::ReadMissingKey) => TreeInstruction::Read(key),
        (true, TreeLogEntry::Read { .. })
        | (true, TreeLogEntry::ReadMissingKey)
        | (false, TreeLogEntry::Inserted)
        | (false, TreeLogEntry::Updated { .. }) => {
            tracing::error!(
                ?storage_log,
                ?tree_log_entry,
                "Failed to map LogQuery to TreeInstruction"
            );
            anyhow::bail!("Failed to map LogQuery to TreeInstruction");
        }
    };

    Ok(tree_instruction)
}

/// Generates the `TreeInstruction`s from the VM executions.
fn generate_tree_instructions(
    mut idx: u64,
    bowp: &BlockOutputWithProofs,
    vm_out: FinishedL1Batch,
) -> anyhow::Result<Vec<TreeInstruction>> {
    vm_out
        .final_execution_state
        .deduplicated_storage_logs
        .into_iter()
        .zip(bowp.logs.iter())
        .map(|(log_query, tree_log_entry)| map_log_tree(&log_query, &tree_log_entry.base, &mut idx))
        .collect::<Result<Vec<_>, _>>()
}

fn execute_tx<S: ReadStorage>(
    tx: &Transaction,
    vm: &mut LegacyVmInstance<S, HistoryEnabled>,
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
        anyhow::bail!("compression can't fail if we don't apply it");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use zksync_contracts::{BaseSystemContracts, SystemContractCode};
    use zksync_multivm::interface::{L1BatchEnv, SystemEnv, TxExecutionMode};
    use zksync_prover_interface::inputs::VMRunWitnessInputData;
    use zksync_tee_prover_interface::inputs::TeeVerifierInput;
    use zksync_types::settlement::SettlementLayer;

    use super::*;

    #[test]
    fn test_v1_serialization() {
        let tvi = V1TeeVerifierInput::new(
            VMRunWitnessInputData {
                l1_batch_number: Default::default(),
                used_bytecodes: Default::default(),
                initial_heap_content: vec![],
                protocol_version: Default::default(),
                bootloader_code: vec![],
                default_account_code_hash: Default::default(),
                evm_emulator_code_hash: Some(Default::default()),
                storage_refunds: vec![],
                pubdata_costs: vec![],
                witness_block_state: Default::default(),
            },
            WitnessInputMerklePaths::new(0),
            vec![],
            L1BatchEnv {
                previous_batch_hash: Some(H256([1; 32])),
                number: Default::default(),
                timestamp: 0,
                fee_input: Default::default(),
                fee_account: Default::default(),
                enforced_base_fee: None,
                first_l2_block: L2BlockEnv {
                    number: 0,
                    timestamp: 0,
                    prev_block_hash: H256([1; 32]),
                    max_virtual_blocks_to_create: 0,
                    interop_roots: vec![],
                },
                settlement_layer: SettlementLayer::for_tests(),
            },
            SystemEnv {
                zk_porter_available: false,
                version: Default::default(),
                base_system_smart_contracts: BaseSystemContracts {
                    bootloader: SystemContractCode {
                        code: vec![1; 32],
                        hash: H256([1; 32]),
                    },
                    default_aa: SystemContractCode {
                        code: vec![1; 32],
                        hash: H256([1; 32]),
                    },
                    evm_emulator: None,
                },
                bootloader_gas_limit: 0,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: 0,
                chain_id: Default::default(),
            },
            Default::default(),
        );
        let tvi = TeeVerifierInput::new(tvi);
        let serialized = bincode::serialize(&tvi).expect("Failed to serialize TeeVerifierInput.");
        let deserialized: TeeVerifierInput =
            bincode::deserialize(&serialized).expect("Failed to deserialize TeeVerifierInput.");

        assert_eq!(tvi, deserialized);
    }
}
