use std::sync::Arc;

use zksync_config::configs::eth_sender::{ProofLoadingMode, ProofSendingMode, SenderConfig};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::StorageProcessor;
use zksync_l1_contract_interface::i_executor::methods::{
    CommitBatches, ExecuteBatches, ProveBatches,
};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::{
    aggregated_operations::AggregatedActionType, commitment::L1BatchWithMetadata,
    helpers::unix_timestamp_ms, protocol_version::L1VerifierConfig, pubdata_da::PubdataDA,
    L1BatchNumber, ProtocolVersionId,
};

use super::{
    aggregated_operations::AggregatedOperation,
    publish_criterion::{
        DataSizeCriterion, GasCriterion, L1BatchPublishCriterion, NumberCriterion,
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
    pubdata_da: PubdataDA,
}

impl Aggregator {
    pub fn new(
        config: SenderConfig,
        blob_store: Arc<dyn ObjectStore>,
        operate_4844_mode: bool,
        pubdata_da: PubdataDA,
    ) -> Self {
        Self {
            commit_criteria: vec![
                Box::from(NumberCriterion {
                    op: AggregatedActionType::Commit,
                    limit: config.max_aggregated_blocks_to_commit,
                }),
                Box::from(GasCriterion::new(
                    AggregatedActionType::Commit,
                    config.max_aggregated_tx_gas,
                )),
                Box::from(DataSizeCriterion {
                    op: AggregatedActionType::Commit,
                    data_limit: config.max_eth_tx_data_size,
                    pubdata_da,
                }),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::Commit,
                    deadline_seconds: config.aggregated_block_commit_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
            ],
            proof_criteria: vec![
                Box::from(NumberCriterion {
                    op: AggregatedActionType::PublishProofOnchain,
                    limit: *config.aggregated_proof_sizes.iter().max().unwrap() as u32,
                }),
                Box::from(GasCriterion::new(
                    AggregatedActionType::PublishProofOnchain,
                    config.max_aggregated_tx_gas,
                )),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::PublishProofOnchain,
                    deadline_seconds: config.aggregated_block_prove_deadline,
                    // Currently, we can't use this functionality for proof criterion
                    // since we don't send dummy and real proofs in the same range,
                    // so even small ranges must be closed.
                    max_allowed_lag: None,
                }),
            ],
            execute_criteria: vec![
                Box::from(NumberCriterion {
                    op: AggregatedActionType::Execute,
                    limit: config.max_aggregated_blocks_to_execute,
                }),
                Box::from(GasCriterion::new(
                    AggregatedActionType::Execute,
                    config.max_aggregated_tx_gas,
                )),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::Execute,
                    deadline_seconds: config.aggregated_block_execute_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
            ],
            config,
            blob_store,
            operate_4844_mode,
            pubdata_da,
        }
    }

    pub async fn get_next_ready_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
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
            .get_proof_operation(
                storage,
                *self.config.aggregated_proof_sizes.iter().max().unwrap(),
                last_sealed_l1_batch_number,
                l1_verifier_config,
            )
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
            .map(AggregatedOperation::Commit)
        }
    }

    async fn get_execute_operations(
        &mut self,
        storage: &mut StorageProcessor<'_>,
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
        .await;

        l1_batches.map(|l1_batches| ExecuteBatches { l1_batches })
    }

    async fn get_commit_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        limit: usize,
        last_sealed_batch: L1BatchNumber,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version_id: ProtocolVersionId,
    ) -> Option<CommitBatches> {
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

        batches.map(|batches| CommitBatches {
            last_committed_l1_batch,
            l1_batches: batches,
            pubdata_da: self.pubdata_da,
        })
    }

    async fn load_dummy_proof_operations(
        storage: &mut StorageProcessor<'_>,
        limit: usize,
        is_4844_mode: bool,
    ) -> Vec<L1BatchWithMetadata> {
        let mut ready_for_proof_l1_batches = storage
            .blocks_dal()
            .get_ready_for_dummy_proof_l1_batches(limit)
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
        storage: &mut StorageProcessor<'_>,
        l1_verifier_config: L1VerifierConfig,
        proof_loading_mode: &ProofLoadingMode,
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

        if let Some(version_id) = storage
            .blocks_dal()
            .get_batch_protocol_version_id(batch_to_prove)
            .await
            .unwrap()
        {
            let verifier_config_for_next_batch = storage
                .protocol_versions_dal()
                .l1_verifier_config_for_version(version_id)
                .await
                .unwrap();
            if verifier_config_for_next_batch != l1_verifier_config {
                return None;
            }
        }
        let proofs = match proof_loading_mode {
            ProofLoadingMode::OldProofFromDb => {
                unreachable!("OldProofFromDb is not supported anymore")
            }
            ProofLoadingMode::FriProofFromGcs => {
                load_wrapped_fri_proofs_for_range(batch_to_prove, batch_to_prove, blob_store).await
            }
        };
        if proofs.is_empty() {
            // The proof for the next L1 batch is not generated yet
            return None;
        }

        assert_eq!(proofs.len(), 1);

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
            proofs,
            should_verify: true,
        })
    }

    async fn prepare_dummy_proof_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
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
        storage: &mut StorageProcessor<'_>,
        limit: usize,
        last_sealed_l1_batch: L1BatchNumber,
        l1_verifier_config: L1VerifierConfig,
    ) -> Option<ProveBatches> {
        match self.config.proof_sending_mode {
            ProofSendingMode::OnlyRealProofs => {
                Self::load_real_proof_operation(
                    storage,
                    l1_verifier_config,
                    &self.config.proof_loading_mode,
                    &*self.blob_store,
                    self.operate_4844_mode,
                )
                .await
            }

            ProofSendingMode::SkipEveryProof => {
                let ready_for_proof_l1_batches =
                    Self::load_dummy_proof_operations(storage, limit, self.operate_4844_mode).await;
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
                    &self.config.proof_loading_mode,
                    &*self.blob_store,
                    self.operate_4844_mode,
                )
                .await
                {
                    Some(op)
                } else {
                    let ready_for_proof_batches = storage
                        .blocks_dal()
                        .get_skipped_for_proof_l1_batches(limit)
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

    pub fn pubdata_da(&self) -> PubdataDA {
        self.pubdata_da
    }
}

async fn extract_ready_subrange(
    storage: &mut StorageProcessor<'_>,
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
    from: L1BatchNumber,
    to: L1BatchNumber,
    blob_store: &dyn ObjectStore,
) -> Vec<L1BatchProofForL1> {
    let mut proofs = Vec::new();
    for l1_batch_number in from.0..=to.0 {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        match blob_store.get(l1_batch_number).await {
            Ok(proof) => proofs.push(proof),
            Err(ObjectStoreError::KeyNotFound(_)) => (), // do nothing, proof is not ready yet
            Err(err) => panic!(
                "Failed to load proof for batch {}: {}",
                l1_batch_number.0, err
            ),
        }
    }
    proofs
}
