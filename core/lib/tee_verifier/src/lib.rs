//! Tee verifier
//!
//! Verifies that a L1Batch has the expected root hash after
//! executing the VM and verifying all the accessed memory slots by their
//! merkle path.

use std::{cell::RefCell, rc::Rc};

use anyhow::Context;
use multivm::{
    interface::{FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmInterface},
    vm_latest::HistoryEnabled,
    VmInstance,
};
use serde::{Deserialize, Serialize};
use vm_utils::execute_tx;
use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    BlockOutputWithProofs, TreeInstruction, TreeLogEntry, TreeLogEntryWithProof,
};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_prover_interface::inputs::{PrepareBasicCircuitsJob, StorageLogMetadata};
use zksync_state::{InMemoryStorage, StorageView, WriteStorage};
use zksync_types::{
    block::L2BlockExecutionData, ethabi::ethereum_types::BigEndianHash, zk_evm_types::LogQuery,
    AccountTreeId, L1BatchNumber, StorageKey, H256,
};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

/// Version 1 of the data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct V1TeeVerifierInput {
    prepare_basic_circuits_job: PrepareBasicCircuitsJob,
    l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
    used_contracts: Vec<(H256, Vec<u8>)>,
}

/// Data used as input for the TEE verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum TeeVerifierInput {
    /// `V0` suppresses warning about irrefutable `let...else` pattern
    V0,
    V1(V1TeeVerifierInput),
}

impl TeeVerifierInput {
    pub fn new(
        prepare_basic_circuits_job: PrepareBasicCircuitsJob,
        l2_blocks_execution_data: Vec<L2BlockExecutionData>,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        used_contracts: Vec<(H256, Vec<u8>)>,
    ) -> Self {
        TeeVerifierInput::V1(V1TeeVerifierInput {
            prepare_basic_circuits_job,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            used_contracts,
        })
    }

    /// Verify that the L1Batch produces the expected root hash
    /// by executing the VM and verifying the merkle paths of all
    /// touch storage slots.
    ///
    /// # Errors
    ///
    /// Returns a verbose error of the failure, because any error is
    /// not actionable.
    pub fn verify(self) -> anyhow::Result<()> {
        let TeeVerifierInput::V1(V1TeeVerifierInput {
            prepare_basic_circuits_job,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            used_contracts,
        }) = self
        else {
            tracing::error!("TeeVerifierInput variant not supported");
            anyhow::bail!("TeeVerifierInput variant not supported");
        };

        let old_root_hash = l1_batch_env.previous_batch_hash.unwrap();
        let l2_chain_id = system_env.chain_id;
        let enumeration_index = prepare_basic_circuits_job.next_enumeration_index();

        let mut raw_storage = InMemoryStorage::with_custom_system_contracts_and_chain_id(
            l2_chain_id,
            hash_bytecode,
            Vec::with_capacity(0),
        );

        for (hash, bytes) in used_contracts.into_iter() {
            tracing::trace!("raw_storage.store_factory_dep({hash}, bytes)");
            raw_storage.store_factory_dep(hash, bytes)
        }

        let block_output_with_proofs =
            Self::get_bowp_and_set_initial_values(prepare_basic_circuits_job, &mut raw_storage);

        let storage_view = Rc::new(RefCell::new(StorageView::new(&raw_storage)));

        let vm = VmInstance::new(l1_batch_env, system_env, storage_view);

        let vm_out = Self::execute_vm(l2_blocks_execution_data, vm)?;

        let instructions: Vec<TreeInstruction> =
            Self::generate_tree_instructions(enumeration_index, &block_output_with_proofs, vm_out)?;

        block_output_with_proofs
            .verify_proofs(&Blake2Hasher, old_root_hash, &instructions)
            .context("Failed to verify_proofs {l1_batch_number} correctly!")?;

        Ok(())
    }

    /// Sets the initial storage values and returns `BlockOutputWithProofs`
    fn get_bowp_and_set_initial_values(
        prepare_basic_circuits_job: PrepareBasicCircuitsJob,
        raw_storage: &mut InMemoryStorage,
    ) -> BlockOutputWithProofs {
        let logs = prepare_basic_circuits_job
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
                        (false, _, _) => {
                            // This is a special U256 here, which needs `to_little_endian`
                            let mut hashed_key = [0_u8; 32];
                            leaf_storage_key.to_little_endian(&mut hashed_key);
                            raw_storage.set_value_hashed_enum(
                                hashed_key.into(),
                                leaf_enumeration_index,
                                value_read.into(),
                            );
                            TreeLogEntry::Read {
                                leaf_index: leaf_enumeration_index,
                                value: value_read.into(),
                            }
                        }
                        (true, true, _) => TreeLogEntry::Inserted,
                        (true, false, _) => {
                            // This is a special U256 here, which needs `to_little_endian`
                            let mut hashed_key = [0_u8; 32];
                            leaf_storage_key.to_little_endian(&mut hashed_key);
                            raw_storage.set_value_hashed_enum(
                                hashed_key.into(),
                                leaf_enumeration_index,
                                value_read.into(),
                            );
                            TreeLogEntry::Updated {
                                leaf_index: leaf_enumeration_index,
                                previous_value: value_read.into(),
                            }
                        }
                    };
                    TreeLogEntryWithProof {
                        base,
                        merkle_path,
                        root_hash,
                    }
                },
            )
            .collect();

        BlockOutputWithProofs {
            logs,
            leaf_count: 0,
        }
    }

    /// Executes the VM and returns `FinishedL1Batch` on success.
    fn execute_vm<S: WriteStorage>(
        l2_blocks_execution_data: Vec<L2BlockExecutionData>,
        mut vm: VmInstance<S, HistoryEnabled>,
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
            vm.start_new_l2_block(L2BlockEnv::from_l2_block_data(next_l2_block_data));

            tracing::trace!("Finished execution of l2_block: {:?}", l2_block_data.number);
        }

        Ok(vm.finish_batch())
    }

    /// Map `LogQuery` and `TreeLogEntry` to a `TreeInstruction`
    fn map_log_tree(
        log_query: &LogQuery,
        tree_log_entry: &TreeLogEntry,
        idx: &mut u64,
    ) -> anyhow::Result<TreeInstruction> {
        let key = StorageKey::new(
            AccountTreeId::new(log_query.address),
            u256_to_h256(log_query.key),
        )
        .hashed_key_u256();
        Ok(match (log_query.rw_flag, *tree_log_entry) {
            (true, TreeLogEntry::Updated { leaf_index, .. }) => {
                TreeInstruction::write(key, leaf_index, H256(log_query.written_value.into()))
            }
            (true, TreeLogEntry::Inserted) => {
                let leaf_index = *idx;
                *idx += 1;
                TreeInstruction::write(key, leaf_index, H256(log_query.written_value.into()))
            }
            (false, TreeLogEntry::Read { value, .. }) => {
                if log_query.read_value != value.into_uint() {
                    tracing::error!(
                        "Failed to map LogQuery to TreeInstruction: {:#?} != {:#?}",
                        log_query.read_value,
                        value
                    );
                    anyhow::bail!(
                        "Failed to map LogQuery to TreeInstruction: {:#?} != {:#?}",
                        log_query.read_value,
                        value
                    );
                }
                TreeInstruction::Read(key)
            }
            (false, TreeLogEntry::ReadMissingKey { .. }) => TreeInstruction::Read(key),
            _ => {
                tracing::error!("Failed to map LogQuery to TreeInstruction");
                anyhow::bail!("Failed to map LogQuery to TreeInstruction");
            }
        })
    }

    /// Generates the `TreeInstruction`s from the VM executions.
    fn generate_tree_instructions(
        mut idx: u64,
        bowp: &BlockOutputWithProofs,
        vm_out: FinishedL1Batch,
    ) -> anyhow::Result<Vec<TreeInstruction>> {
        vm_out
            .final_execution_state
            .deduplicated_storage_log_queries
            .into_iter()
            .zip(bowp.logs.iter())
            .map(|(log_query, tree_log_entry)| {
                Self::map_log_tree(&log_query, &tree_log_entry.base, &mut idx)
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

impl StoredObject for TeeVerifierInput {
    const BUCKET: Bucket = Bucket::TeeVerifierInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("tee_verifier_input_for_l1_batch_{key}.bin")
    }

    serialize_using_bincode!();
}

#[cfg(test)]
mod tests {
    use multivm::interface::TxExecutionMode;
    use zksync_basic_types::U256;
    use zksync_contracts::{BaseSystemContracts, SystemContractCode};

    use super::*;

    #[test]
    fn test_v1_serialization() {
        let tvi = TeeVerifierInput::new(
            PrepareBasicCircuitsJob::new(0),
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
                },
            },
            SystemEnv {
                zk_porter_available: false,
                version: Default::default(),
                base_system_smart_contracts: BaseSystemContracts {
                    bootloader: SystemContractCode {
                        code: vec![U256([1; 4])],
                        hash: H256([1; 32]),
                    },
                    default_aa: SystemContractCode {
                        code: vec![U256([1; 4])],
                        hash: H256([1; 32]),
                    },
                },
                bootloader_gas_limit: 0,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: 0,
                chain_id: Default::default(),
            },
            vec![(H256([1; 32]), vec![0, 1, 2, 3, 4])],
        );

        let serialized = <TeeVerifierInput as StoredObject>::serialize(&tvi)
            .expect("Failed to serialize TeeVerifierInput.");
        let deserialized: TeeVerifierInput =
            <TeeVerifierInput as StoredObject>::deserialize(serialized)
                .expect("Failed to deserialize TeeVerifierInput.");

        assert_eq!(tvi, deserialized);
    }
}
