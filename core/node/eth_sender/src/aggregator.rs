use std::sync::Arc;

use zksync_config::configs::eth_sender::{ProofSendingMode, SenderConfig};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata, PriorityOpsMerkleProof},
    hasher::keccak::KeccakHasher,
    helpers::unix_timestamp_ms,
    l1::L1Tx,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    pubdata_da::PubdataSendingMode,
    settlement::SettlementMode,
    Address, L1BatchNumber, ProtocolVersionId,
};

use super::{
    aggregated_operations::AggregatedOperation,
    publish_criterion::{
        GasCriterionKind, L1BatchPublishCriterion, L1GasCriterion, NumberCriterion,
        TimestampDeadlineCriterion,
    },
};

#[derive(Debug)]
pub struct Aggregator {
    commit_criteria: Vec<Box<dyn L1BatchPublishCriterion>>,
    proof_criteria: Vec<Box<dyn L1BatchPublishCriterion>>,
    execute_criteria: Vec<Box<dyn L1BatchPublishCriterion>>,
    config: SenderConfig,
    blob_store: Arc<dyn ObjectStore>,
    /// If we are operating in 4844 mode we need to wait for commit transaction
    /// to get included before sending the respective prove and execute transactions.
    /// In non-4844 mode of operation we operate with the single address and this
    /// means no wait is needed: nonces will still provide the correct ordering of
    /// transactions.
    operate_4844_mode: bool,
    pubdata_da: PubdataSendingMode,
    commitment_mode: L1BatchCommitmentMode,
    priority_merkle_tree: MiniMerkleTree<L1Tx>,
}

impl Aggregator {
    pub async fn new(
        config: SenderConfig,
        blob_store: Arc<dyn ObjectStore>,
        custom_commit_sender_addr: Option<Address>,
        commitment_mode: L1BatchCommitmentMode,
        connection: &mut Connection<'_, Core>,
        settlement_mode: SettlementMode,
    ) -> anyhow::Result<Self> {
        let pubdata_da = config.pubdata_sending_mode;

        let priority_tree_start_index = config.priority_tree_start_index.unwrap_or(0);
        let priority_op_hashes = connection
            .transactions_dal()
            .get_l1_transactions_hashes(priority_tree_start_index)
            .await
            .map_err(DalError::generalize)?;
        let priority_merkle_tree =
            MiniMerkleTree::<L1Tx>::from_hashes(KeccakHasher, priority_op_hashes.into_iter(), None);

        let operate_4844_mode =
            custom_commit_sender_addr.is_some() && !settlement_mode.is_gateway();

        // We do not have a reliable lower bound for gas needed to execute batches on gateway so we do not aggregate.
        let execute_criteria: Vec<Box<dyn L1BatchPublishCriterion>> = if settlement_mode
            .is_gateway()
        {
            if config.max_aggregated_blocks_to_execute > 1 {
                tracing::warn!(
                    "config.max_aggregated_blocks_to_execute is set to {} but \
                    aggregator does not support aggregating execute operations when settling on gateway",
                    config.max_aggregated_blocks_to_execute
                );
            }

            vec![Box::from(NumberCriterion {
                op: AggregatedActionType::Execute,
                limit: 1,
            })]
        } else {
            vec![
                Box::from(NumberCriterion {
                    op: AggregatedActionType::Execute,
                    limit: config.max_aggregated_blocks_to_execute,
                }),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::Execute,
                    deadline_seconds: config.aggregated_block_execute_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
                Box::from(L1GasCriterion::new(
                    config.max_aggregated_tx_gas,
                    GasCriterionKind::Execute,
                )),
            ]
        };

        // It only makes sense to aggregate commit operation when validium chain settles to L1.
        let commit_criteria: Vec<Box<dyn L1BatchPublishCriterion>> = if settlement_mode
            == SettlementMode::SettlesToL1
            && commitment_mode == L1BatchCommitmentMode::Validium
        {
            vec![
                Box::from(NumberCriterion {
                    op: AggregatedActionType::Commit,
                    limit: config.max_aggregated_blocks_to_commit,
                }),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::Commit,
                    deadline_seconds: config.aggregated_block_commit_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
                Box::from(L1GasCriterion::new(
                    config.max_aggregated_tx_gas,
                    GasCriterionKind::CommitValidium,
                )),
            ]
        } else {
            if config.max_aggregated_blocks_to_commit > 1 {
                tracing::warn!(
                    "config.max_aggregated_blocks_to_commit is set to {} but \
                    aggregator does not support aggregating commit operations anymore",
                    config.max_aggregated_blocks_to_commit
                );
            }
            vec![Box::from(NumberCriterion {
                op: AggregatedActionType::Commit,
                limit: 1,
            })]
        };

        Ok(Self {
            commit_criteria,
            proof_criteria: vec![Box::from(NumberCriterion {
                op: AggregatedActionType::PublishProofOnchain,
                limit: 1,
            })],
            execute_criteria,
            config,
            blob_store,
            operate_4844_mode,
            pubdata_da,
            commitment_mode,
            priority_merkle_tree,
        })
    }

    pub async fn get_next_ready_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version_id: ProtocolVersionId,
        l1_verifier_config: L1VerifierConfig,
    ) -> Option<AggregatedOperation> {
        let Some(last_sealed_l1_batch_number) = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap()
        else {
            return None; // No L1 batches in Postgres; no operations are ready yet
        };

        if let Some(op) = self
            .get_execute_operations(
                storage,
                self.config.max_aggregated_blocks_to_execute as usize,
                last_sealed_l1_batch_number,
            )
            .await
        {
            Some(AggregatedOperation::Execute(op))
        } else if let Some(op) = self
            .get_proof_operation(storage, last_sealed_l1_batch_number, l1_verifier_config)
            .await
        {
            Some(AggregatedOperation::PublishProofOnchain(op))
        } else {
            self.get_commit_operation(
                storage,
                self.config.max_aggregated_blocks_to_commit as usize,
                last_sealed_l1_batch_number,
                base_system_contracts_hashes,
                protocol_version_id,
            )
            .await
        }
    }

    async fn get_execute_operations(
        &mut self,
        storage: &mut Connection<'_, Core>,
        limit: usize,
        last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<ExecuteBatches> {
        let max_l1_batch_timestamp_millis = self
            .config
            .l1_batch_min_age_before_execute_seconds
            .map(|age| unix_timestamp_ms() - age * 1_000);
        let ready_for_execute_batches = storage
            .blocks_dal()
            .get_ready_for_execute_l1_batches(limit, max_l1_batch_timestamp_millis)
            .await
            .unwrap();
        let l1_batches = extract_ready_subrange(
            storage,
            &mut self.execute_criteria,
            ready_for_execute_batches,
            last_sealed_l1_batch,
        )
        .await?;

        let priority_tree_start_index = self.config.priority_tree_start_index.unwrap_or(0);
        let mut priority_ops_proofs = vec![];
        for batch in l1_batches.iter() {
            let first_priority_op_id_option = match storage
                .blocks_dal()
                .get_batch_first_priority_op_id(batch.header.number)
                .await
                .unwrap()
            {
                // Batch has no priority ops, no proofs to send
                None => None,
                // We haven't started to use the priority tree in the contracts yet
                Some(id) if id < priority_tree_start_index => None,
                Some(id) => Some(id),
            };

            let count = batch.header.l1_tx_count as usize;
            if let Some(first_priority_op_id_in_batch) = first_priority_op_id_option {
                let priority_tree_start_index = self.config.priority_tree_start_index.unwrap_or(0);
                let new_l1_tx_hashes = storage
                    .transactions_dal()
                    .get_l1_transactions_hashes(
                        priority_tree_start_index + self.priority_merkle_tree.length(),
                    )
                    .await
                    .unwrap();
                for hash in new_l1_tx_hashes {
                    self.priority_merkle_tree.push_hash(hash);
                }

                self.priority_merkle_tree.trim_start(
                    first_priority_op_id_in_batch // global index
                        - priority_tree_start_index // first index when tree is activated
                        - self.priority_merkle_tree.start_index(), // first index in the tree
                );
                let (_, left, right) = self
                    .priority_merkle_tree
                    .merkle_root_and_paths_for_range(..count);
                let hashes = self.priority_merkle_tree.hashes_prefix(count);
                priority_ops_proofs.push(PriorityOpsMerkleProof {
                    left_path: left.into_iter().map(Option::unwrap_or_default).collect(),
                    right_path: right.into_iter().map(Option::unwrap_or_default).collect(),
                    hashes,
                });
            } else {
                priority_ops_proofs.push(Default::default());
            }
        }

        Some(ExecuteBatches {
            l1_batches,
            priority_ops_proofs,
        })
    }

    async fn get_commit_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        limit: usize,
        last_sealed_batch: L1BatchNumber,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version_id: ProtocolVersionId,
    ) -> Option<AggregatedOperation> {
        let mut blocks_dal = storage.blocks_dal();
        let last_committed_l1_batch = blocks_dal
            .get_last_committed_to_eth_l1_batch()
            .await
            .unwrap()?;

        let ready_for_commit_l1_batches = if protocol_version_id.is_pre_boojum() {
            blocks_dal
                .pre_boojum_get_ready_for_commit_l1_batches(
                    limit,
                    base_system_contracts_hashes.bootloader,
                    base_system_contracts_hashes.default_aa,
                    protocol_version_id,
                )
                .await
                .unwrap()
        } else {
            blocks_dal
                .get_ready_for_commit_l1_batches(
                    limit,
                    base_system_contracts_hashes.bootloader,
                    base_system_contracts_hashes.default_aa,
                    protocol_version_id,
                    self.commitment_mode != L1BatchCommitmentMode::Rollup,
                )
                .await
                .unwrap()
        };

        // Check that the L1 batches that are selected are sequential
        ready_for_commit_l1_batches
            .iter()
            .reduce(|last_batch, next_batch| {
                if last_batch.header.number + 1 == next_batch.header.number {
                    next_batch
                } else {
                    panic!("L1 batches prepared for commit are not sequential");
                }
            });

        let batches = extract_ready_subrange(
            storage,
            &mut self.commit_criteria,
            ready_for_commit_l1_batches,
            last_sealed_batch,
        )
        .await;

        batches.map(|batches| {
            AggregatedOperation::Commit(last_committed_l1_batch, batches, self.pubdata_da)
        })
    }

    async fn load_dummy_proof_operations(
        storage: &mut Connection<'_, Core>,
        is_4844_mode: bool,
    ) -> Vec<L1BatchWithMetadata> {
        let mut ready_for_proof_l1_batches = storage
            .blocks_dal()
            .get_ready_for_dummy_proof_l1_batches(1)
            .await
            .unwrap();

        // need to find first batch with an unconfirmed commit transaction
        // and discard it and all the following ones.
        if is_4844_mode {
            let mut committed_batches = vec![];

            for batch in ready_for_proof_l1_batches.into_iter() {
                let Some(commit_tx_id) = storage
                    .blocks_dal()
                    .get_eth_commit_tx_id(batch.header.number)
                    .await
                    .unwrap()
                else {
                    break;
                };

                if storage
                    .eth_sender_dal()
                    .get_confirmed_tx_hash_by_eth_tx_id(commit_tx_id as u32)
                    .await
                    .unwrap()
                    .is_none()
                {
                    break;
                }
                committed_batches.push(batch);
            }

            ready_for_proof_l1_batches = committed_batches;
        }

        ready_for_proof_l1_batches
    }

    async fn load_real_proof_operation(
        storage: &mut Connection<'_, Core>,
        l1_verifier_config: L1VerifierConfig,
        blob_store: &dyn ObjectStore,
        is_4844_mode: bool,
    ) -> Option<ProveBatches> {
        let previous_proven_batch_number = storage
            .blocks_dal()
            .get_last_l1_batch_with_prove_tx()
            .await
            .unwrap();
        let batch_to_prove = previous_proven_batch_number + 1;

        // Return `None` if batch is not committed yet.
        let commit_tx_id = storage
            .blocks_dal()
            .get_eth_commit_tx_id(batch_to_prove)
            .await
            .unwrap()?;

        if is_4844_mode
            && storage
                .eth_sender_dal()
                .get_confirmed_tx_hash_by_eth_tx_id(commit_tx_id as u32)
                .await
                .unwrap()
                .is_none()
        {
            return None;
        }

        let minor_version = storage
            .blocks_dal()
            .get_batch_protocol_version_id(batch_to_prove)
            .await
            .unwrap()
            .unwrap();

        // `l1_verifier_config.recursion_scheduler_level_vk_hash` is a VK hash that L1 uses.
        // We may have multiple versions with different verification keys, so we check only for proofs that use
        // keys that correspond to one on L1.
        let allowed_patch_versions = storage
            .protocol_versions_dal()
            .get_patch_versions_for_vk(minor_version, l1_verifier_config.snark_wrapper_vk_hash)
            .await
            .unwrap();
        if allowed_patch_versions.is_empty() {
            tracing::warn!(
                "No patch version corresponds to the verification key on L1: {:?}",
                l1_verifier_config.snark_wrapper_vk_hash
            );
            return None;
        };

        let allowed_versions: Vec<_> = allowed_patch_versions
            .into_iter()
            .map(|patch| ProtocolSemanticVersion {
                minor: minor_version,
                patch,
            })
            .collect();

        let proof =
            load_wrapped_fri_proofs_for_range(batch_to_prove, blob_store, &allowed_versions).await;
        let Some(proof) = proof else {
            // The proof for the next L1 batch is not generated yet
            return None;
        };

        let previous_proven_batch_metadata = storage
            .blocks_dal()
            .get_l1_batch_metadata(previous_proven_batch_number)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "L1 batch #{} with submitted proof is not complete in the DB",
                    previous_proven_batch_number
                );
            });
        let metadata_for_batch_being_proved = storage
            .blocks_dal()
            .get_l1_batch_metadata(previous_proven_batch_number + 1)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "L1 batch #{} with generated proof is not complete in the DB",
                    previous_proven_batch_number + 1
                );
            });

        Some(ProveBatches {
            prev_l1_batch: previous_proven_batch_metadata,
            l1_batches: vec![metadata_for_batch_being_proved],
            proofs: vec![proof],
            should_verify: true,
        })
    }

    async fn prepare_dummy_proof_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        ready_for_proof_l1_batches: Vec<L1BatchWithMetadata>,
        last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<ProveBatches> {
        let batches = extract_ready_subrange(
            storage,
            &mut self.proof_criteria,
            ready_for_proof_l1_batches,
            last_sealed_l1_batch,
        )
        .await?;

        let prev_l1_batch_number = batches.first().map(|batch| batch.header.number - 1)?;
        let prev_batch = storage
            .blocks_dal()
            .get_l1_batch_metadata(prev_l1_batch_number)
            .await
            .unwrap()?;

        Some(ProveBatches {
            prev_l1_batch: prev_batch,
            l1_batches: batches,
            proofs: vec![],
            should_verify: false,
        })
    }

    async fn get_proof_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        last_sealed_l1_batch: L1BatchNumber,
        l1_verifier_config: L1VerifierConfig,
    ) -> Option<ProveBatches> {
        match self.config.proof_sending_mode {
            ProofSendingMode::OnlyRealProofs => {
                Self::load_real_proof_operation(
                    storage,
                    l1_verifier_config,
                    &*self.blob_store,
                    self.operate_4844_mode,
                )
                .await
            }

            ProofSendingMode::SkipEveryProof => {
                let ready_for_proof_l1_batches =
                    Self::load_dummy_proof_operations(storage, self.operate_4844_mode).await;
                self.prepare_dummy_proof_operation(
                    storage,
                    ready_for_proof_l1_batches,
                    last_sealed_l1_batch,
                )
                .await
            }

            ProofSendingMode::OnlySampledProofs => {
                // if there is a sampled proof then send it, otherwise check for skipped ones.
                if let Some(op) = Self::load_real_proof_operation(
                    storage,
                    l1_verifier_config,
                    &*self.blob_store,
                    self.operate_4844_mode,
                )
                .await
                {
                    Some(op)
                } else {
                    let ready_for_proof_batches = storage
                        .blocks_dal()
                        .get_skipped_for_proof_l1_batches(1)
                        .await
                        .unwrap();
                    self.prepare_dummy_proof_operation(
                        storage,
                        ready_for_proof_batches,
                        last_sealed_l1_batch,
                    )
                    .await
                }
            }
        }
    }

    pub fn pubdata_da(&self) -> PubdataSendingMode {
        self.pubdata_da
    }

    pub fn mode(&self) -> L1BatchCommitmentMode {
        self.commitment_mode
    }
}

async fn extract_ready_subrange(
    storage: &mut Connection<'_, Core>,
    publish_criteria: &mut [Box<dyn L1BatchPublishCriterion>],
    unpublished_l1_batches: Vec<L1BatchWithMetadata>,
    last_sealed_l1_batch: L1BatchNumber,
) -> Option<Vec<L1BatchWithMetadata>> {
    let mut last_l1_batch: Option<L1BatchNumber> = None;
    for criterion in publish_criteria {
        let l1_batch_by_criterion = criterion
            .last_l1_batch_to_publish(storage, &unpublished_l1_batches, last_sealed_l1_batch)
            .await;
        if let Some(l1_batch) = l1_batch_by_criterion {
            last_l1_batch = Some(last_l1_batch.map_or(l1_batch, |number| number.min(l1_batch)));
        }
    }

    let last_l1_batch = last_l1_batch?;
    Some(
        unpublished_l1_batches
            .into_iter()
            .take_while(|l1_batch| l1_batch.header.number <= last_l1_batch)
            .collect(),
    )
}

pub async fn load_wrapped_fri_proofs_for_range(
    l1_batch_number: L1BatchNumber,
    blob_store: &dyn ObjectStore,
    allowed_versions: &[ProtocolSemanticVersion],
) -> Option<L1BatchProofForL1> {
    for version in allowed_versions {
        match blob_store.get((l1_batch_number, *version)).await {
            Ok(proof) => return Some(proof),
            Err(ObjectStoreError::KeyNotFound(_)) => (), // do nothing, proof is not ready yet
            Err(err) => panic!(
                "Failed to load proof for batch {}: {}",
                l1_batch_number.0, err
            ),
        }
    }

    None
}
