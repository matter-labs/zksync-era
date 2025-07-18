use std::sync::Arc;

use chrono::Utc;
use zksync_config::configs::eth_sender::{PrecommitParams, ProofSendingMode, SenderConfig};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::{
    outputs::{L1BatchProofForL1, L1BatchProofForL1Key},
    Bincode,
};
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType,
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata, PriorityOpsMerkleProof},
    hasher::keccak::KeccakHasher,
    helpers::unix_timestamp_ms,
    l1::L1Tx,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    pubdata_da::PubdataSendingMode,
    settlement::SettlementLayer,
    transaction_status_commitment::TransactionStatusCommitment,
    InteropRoot, L1BatchNumber, ProtocolVersionId,
};

use super::{
    aggregated_operations::AggregatedOperation,
    publish_criterion::{
        GasCriterionKind, L1BatchPublishCriterion, L1GasCriterion, NumberCriterion,
        TimestampDeadlineCriterion,
    },
};
use crate::{
    aggregated_operations::{L1BatchAggregatedOperation, L2BlockAggregatedOperation},
    EthSenderError,
};

#[derive(Debug)]
pub struct Aggregator {
    commit_criteria: Vec<Box<dyn L1BatchPublishCriterion>>,
    proof_criteria: Vec<Box<dyn L1BatchPublishCriterion>>,
    execute_criteria: Vec<Box<dyn L1BatchPublishCriterion>>,
    config: SenderConfig,
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    /// If we are operating in 4844 mode we need to wait for commit transaction
    /// to get included before sending the respective prove and execute transactions.
    /// In non-4844 mode of operation we operate with the single address and this
    /// means no wait is needed: nonces will still provide the correct ordering of
    /// transactions.
    operate_4844_mode: bool,
    pubdata_da: PubdataSendingMode,
    commitment_mode: L1BatchCommitmentMode,
    priority_merkle_tree: Option<MiniMerkleTree<L1Tx>>,
    settlement_layer: SettlementLayer,
}

/// Denotes whether there are any restrictions on sending either
/// commit, prove or execute operations. If there is one, the reason for it
/// is stored to be logged.
#[derive(Debug, Default)]
pub(crate) struct OperationSkippingRestrictions {
    pub(crate) commit_restriction: Option<&'static str>,
    pub(crate) prove_restriction: Option<&'static str>,
    pub(crate) execute_restriction: Option<&'static str>,
    pub(crate) precommit_restriction: Option<&'static str>,
}

impl OperationSkippingRestrictions {
    fn check_for_continuation(
        &self,
        agg_op: &L1BatchAggregatedOperation,
        reason: Option<&'static str>,
    ) -> bool {
        if let Some(reason) = reason {
            tracing::info!(
                "Skipping sending operation of type {} for batches {}-{} since {}",
                agg_op.get_action_type(),
                agg_op.l1_batch_range().start(),
                agg_op.l1_batch_range().end(),
                reason
            );
            false
        } else {
            true
        }
    }

    // Unlike other funcitons `filter_commit_op` accepts an already prepared `AggregatedOperation` for
    // easier compatibility with other interfaces in the file.
    fn filter_commit_op(
        &self,
        commit_op: Option<L1BatchAggregatedOperation>,
    ) -> Option<AggregatedOperation> {
        let commit_op = commit_op?;
        self.check_for_continuation(&commit_op, self.commit_restriction)
            .then_some(AggregatedOperation::L1Batch(commit_op))
    }

    fn filter_prove_op(&self, prove_op: Option<ProveBatches>) -> Option<AggregatedOperation> {
        let op = L1BatchAggregatedOperation::PublishProofOnchain(prove_op?);
        self.check_for_continuation(&op, self.prove_restriction)
            .then_some(AggregatedOperation::L1Batch(op))
    }

    fn filter_execute_op(&self, execute_op: Option<ExecuteBatches>) -> Option<AggregatedOperation> {
        let op = L1BatchAggregatedOperation::Execute(execute_op?);
        self.check_for_continuation(&op, self.execute_restriction)
            .then_some(AggregatedOperation::L1Batch(op))
    }

    fn filter_precommit_op(
        &self,
        precommit_op: Option<L2BlockAggregatedOperation>,
    ) -> Option<AggregatedOperation> {
        let precommit_op = precommit_op?;
        if let Some(reason) = self.precommit_restriction {
            tracing::info!(
                "Skipping sending operation of type {} for blocks {}-{} since {}",
                precommit_op.get_action_type(),
                precommit_op.l2_blocks_range().start(),
                precommit_op.l2_blocks_range().end(),
                reason
            );
            None
        } else {
            Some(AggregatedOperation::L2Block(precommit_op))
        }
    }
}

impl Aggregator {
    pub async fn new(
        config: SenderConfig,
        blob_store: Arc<dyn ObjectStore>,
        custom_commit_sender_addr: bool,
        commitment_mode: L1BatchCommitmentMode,
        pool: ConnectionPool<Core>,
        settlement_layer: SettlementLayer,
    ) -> anyhow::Result<Self> {
        let operate_4844_mode: bool = custom_commit_sender_addr && !settlement_layer.is_gateway();

        // We do not have a reliable lower bound for gas needed to execute batches on gateway so we do not aggregate.
        let execute_criteria: Vec<Box<dyn L1BatchPublishCriterion>> = if settlement_layer
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
                op: L1BatchAggregatedActionType::Execute,
                limit: 1,
            })]
        } else {
            vec![
                Box::from(NumberCriterion {
                    op: L1BatchAggregatedActionType::Execute,
                    limit: config.max_aggregated_blocks_to_execute,
                }),
                Box::from(TimestampDeadlineCriterion {
                    op: L1BatchAggregatedActionType::Execute,
                    deadline: config.aggregated_block_execute_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
                Box::from(L1GasCriterion::new(
                    config.max_aggregated_tx_gas,
                    GasCriterionKind::Execute,
                )),
            ]
        };

        // It only makes sense to aggregate commit operation when validium chain settles to L1.
        let commit_criteria: Vec<Box<dyn L1BatchPublishCriterion>> =
            if !settlement_layer.is_gateway() && commitment_mode == L1BatchCommitmentMode::Validium
            {
                vec![
                    Box::from(NumberCriterion {
                        op: L1BatchAggregatedActionType::Commit,
                        limit: config.max_aggregated_blocks_to_commit,
                    }),
                    Box::from(TimestampDeadlineCriterion {
                        op: L1BatchAggregatedActionType::Commit,
                        deadline: config.aggregated_block_commit_deadline,
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
                    op: L1BatchAggregatedActionType::Commit,
                    limit: 1,
                })]
            };

        Ok(Self {
            commit_criteria,
            proof_criteria: vec![Box::from(NumberCriterion {
                op: L1BatchAggregatedActionType::PublishProofOnchain,
                limit: 1,
            })],
            execute_criteria,
            pubdata_da: config.pubdata_sending_mode,
            config,
            blob_store,
            operate_4844_mode,
            commitment_mode,
            priority_merkle_tree: None,
            pool,
            settlement_layer,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn get_next_ready_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version_id: ProtocolVersionId,
        l1_verifier_config: L1VerifierConfig,
        restrictions: OperationSkippingRestrictions,
        priority_tree_start_index: Option<usize>,
        precommit_params: Option<&PrecommitParams>,
    ) -> Result<Option<AggregatedOperation>, EthSenderError> {
        let Some(last_sealed_l1_batch_number) = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap()
        else {
            return Ok(None); // No L1 batches in Postgres; no operations are ready yet
        };

        if let Some(op) = restrictions.filter_execute_op(
            self.get_execute_operations(
                storage,
                self.config.max_aggregated_blocks_to_execute as usize,
                last_sealed_l1_batch_number,
                priority_tree_start_index,
            )
            .await?,
        ) {
            Ok(Some(op))
        } else if let Some(op) = restrictions.filter_prove_op(
            self.get_proof_operation(storage, last_sealed_l1_batch_number, l1_verifier_config)
                .await,
        ) {
            Ok(Some(op))
        } else if let Some(op) = restrictions.filter_commit_op(
            self.get_commit_operation(
                storage,
                self.config.max_aggregated_blocks_to_commit as usize,
                last_sealed_l1_batch_number,
                base_system_contracts_hashes,
                protocol_version_id,
                precommit_params.is_some(),
            )
            .await,
        ) {
            Ok(Some(op))
        } else if let Some(params) = precommit_params {
            Ok(restrictions
                .filter_precommit_op(self.get_precommit_operation(storage, params).await?))
        } else {
            Ok(None)
        }
    }

    async fn get_or_init_tree(
        &mut self,
        priority_tree_start_index: usize,
    ) -> &mut MiniMerkleTree<L1Tx> {
        if self.priority_merkle_tree.is_none() {
            // We unwrap here since it is only invoked during initialization
            let mut connection = self.pool.connection_tagged("eth_sender").await.unwrap();

            // We unwrap here since it is only invoked only once during initialization
            let priority_op_hashes = connection
                .transactions_dal()
                .get_l1_transactions_hashes(priority_tree_start_index)
                .await
                .unwrap();
            let priority_merkle_tree = MiniMerkleTree::<L1Tx>::from_hashes(
                KeccakHasher,
                priority_op_hashes.into_iter(),
                None,
            );

            self.priority_merkle_tree = Some(priority_merkle_tree);
        };

        // It is known that the `self.priority_merkle_tree` is initialized, so it is safe to unwrap here
        self.priority_merkle_tree.as_mut().unwrap()
    }

    async fn get_precommit_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        precommit_params: &PrecommitParams,
    ) -> Result<Option<L2BlockAggregatedOperation>, EthSenderError> {
        // The first l1 batch needs to be commited is 1, so it's safe to start precommits from batch 1.
        let last_committed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?;

        let last_committed_finalized_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_finailized_on_eth()
            .await?;

        if last_committed_l1_batch != last_committed_finalized_l1_batch {
            // Last committed L1 batch is not finalized yet, skipping precommit operation. During the transition from not using
            // to using precommit we have to wait for the last committed batch to be finalized.
            // Otherwise we can have a race condition and either precommit or commit operation would fail.
            return Ok(None);
        }

        let l1_batch_for_precommit = last_committed_l1_batch.unwrap_or(L1BatchNumber(0)) + 1;
        let txs = storage
            .blocks_dal()
            .get_ready_for_precommit_txs(l1_batch_for_precommit)
            .await?;

        if txs.is_empty() {
            return Ok(None);
        }

        // Vec of txs is not empty, so we can unwrap it
        let first_tx = txs.first().cloned().unwrap();

        let blocks_range_for_potential_precommits = storage
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_for_precommit)
            .await?;

        // If the potential batch has not been sealed, we just send precommit
        // If it was sealed we check that the first block we want to precommit is from the potential batch.
        if let Some((_, last_block)) = blocks_range_for_potential_precommits {
            if last_block < first_tx.l2block_number {
                return Ok(None);
            }
        }

        let l1_batch_number = first_tx.l1_batch_number;

        // Filter out transactions that are not in the same batch as the first transaction. If we need to precommit more than one batch,
        // we will do it in the next iteration.
        let filtered_txs: Vec<_> = txs
            .into_iter()
            .filter(|tx| tx.l1_batch_number == l1_batch_number)
            .collect();

        let last_tx = filtered_txs.last().unwrap();

        // We can skip precommit if we are sending the precommit for not sealed batch and do some batching.
        // If the batch already sealed we have to send it as soon as possible
        if l1_batch_number.is_none() {
            // We need to check that the first and last L2 blocks are in the same batch

            let first_l2_block_age = Utc::now().timestamp() - first_tx.timestamp;
            if first_l2_block_age < precommit_params.deadline.as_secs() as i64
                && (first_tx.l2block_number.0 - last_tx.l2block_number.0
                    < precommit_params.l2_blocks_to_aggregate)
            {
                return Ok(None);
            }
        }

        Ok(Some(L2BlockAggregatedOperation::Precommit {
            l1_batch: l1_batch_for_precommit,
            first_l2_block: first_tx.l2block_number,
            last_l2_block: last_tx.l2block_number,
            txs: filtered_txs
                .into_iter()
                .map(|tx| TransactionStatusCommitment {
                    tx_hash: tx.tx_hash,
                    is_success: tx.is_success,
                })
                .collect(),
        }))
    }

    async fn get_execute_operations(
        &mut self,
        storage: &mut Connection<'_, Core>,
        limit: usize,
        last_sealed_l1_batch: L1BatchNumber,
        priority_tree_start_index: Option<usize>,
    ) -> Result<Option<ExecuteBatches>, EthSenderError> {
        let max_l1_batch_timestamp_millis = self
            .config
            .l1_batch_min_age_before_execute
            .map(|age| unix_timestamp_ms() - age.as_millis() as u64);
        let ready_for_execute_batches = storage
            .blocks_dal()
            .get_ready_for_execute_l1_batches(limit, max_l1_batch_timestamp_millis)
            .await
            .unwrap();
        let Some(l1_batches) = extract_ready_subrange(
            storage,
            &mut self.execute_criteria,
            ready_for_execute_batches,
            last_sealed_l1_batch,
            self.settlement_layer.is_gateway(),
        )
        .await
        else {
            return Ok(None);
        };

        let mut dependency_roots: Vec<Vec<InteropRoot>> = vec![];
        for batch in &l1_batches {
            let interop_roots = storage
                .interop_root_dal()
                .get_interop_roots_batch(batch.header.number)
                .await
                .unwrap();

            dependency_roots.push(interop_roots);
        }

        let Some(priority_tree_start_index) = priority_tree_start_index else {
            // The index is not yet applicable to the current system, so we
            // return empty priority operations' proofs.
            let length = l1_batches.len();
            return Ok(Some(ExecuteBatches {
                l1_batches,
                priority_ops_proofs: vec![Default::default(); length],
                dependency_roots,
            }));
        };

        let priority_merkle_tree = self.get_or_init_tree(priority_tree_start_index).await;

        let mut priority_ops_proofs = vec![];
        for batch in &l1_batches {
            let first_priority_op_id_option = storage
                .blocks_dal()
                .get_batch_first_priority_op_id(batch.header.number)
                .await
                .unwrap()
                .filter(|id| *id >= priority_tree_start_index);

            let count = batch.header.l1_tx_count as usize;
            if let Some(first_priority_op_id_in_batch) = first_priority_op_id_option {
                let new_l1_tx_hashes = storage
                    .transactions_dal()
                    .get_l1_transactions_hashes(
                        priority_tree_start_index + priority_merkle_tree.length(),
                    )
                    .await
                    .unwrap();
                for hash in new_l1_tx_hashes {
                    priority_merkle_tree.push_hash(hash);
                }

                // We cache paths for priority transactions that happened in the previous batches.
                // For this we absorb all the elements up to `first_priority_op_id_in_batch`.`
                priority_merkle_tree.trim_start(
                    first_priority_op_id_in_batch // global index
                        - priority_tree_start_index // first index when tree is activated
                        - priority_merkle_tree.start_index(), // first index in the tree
                );
                let (_, left, right) =
                    priority_merkle_tree.merkle_root_and_paths_for_range(..count);
                let hashes = priority_merkle_tree.hashes_prefix(count);
                priority_ops_proofs.push(PriorityOpsMerkleProof {
                    left_path: left.into_iter().map(Option::unwrap_or_default).collect(),
                    right_path: right.into_iter().map(Option::unwrap_or_default).collect(),
                    hashes,
                });
            } else {
                priority_ops_proofs.push(Default::default());
            }
        }

        Ok(Some(ExecuteBatches {
            l1_batches,
            priority_ops_proofs,
            dependency_roots,
        }))
    }

    async fn get_commit_operation(
        &mut self,
        storage: &mut Connection<'_, Core>,
        limit: usize,
        last_sealed_batch: L1BatchNumber,
        base_system_contracts_hashes: BaseSystemContractsHashes,
        protocol_version_id: ProtocolVersionId,
        send_precommit_tx: bool,
    ) -> Option<L1BatchAggregatedOperation> {
        // The commit operation is not aggregated at the moment. The code below relies on `limit`
        // being set to 1 when defining the pubdata commitment mode.
        if limit != 1 {
            tracing::error!(
                "Commit operation is not aggregated anymore. \
                The limit of commit operation is set to 1."
            );
            return None;
        }

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
                    send_precommit_tx,
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
            self.settlement_layer.is_gateway(),
        )
        .await;

        let batches = batches?;

        // Note: the line below only works correctly during rollup <-> validium transitions
        // if the limit of commit operation is set to 1.
        let (pubdata_sending_mode, commitment_mode) =
            self.get_commitment_modes(batches.first()?, storage).await;

        Some(L1BatchAggregatedOperation::Commit(
            last_committed_l1_batch,
            batches,
            pubdata_sending_mode,
            commitment_mode,
        ))
    }

    async fn get_commitment_modes(
        &self,
        batch: &L1BatchWithMetadata,
        storage: &mut Connection<'_, Core>,
    ) -> (PubdataSendingMode, L1BatchCommitmentMode) {
        let pubdata_params = storage
            .blocks_dal()
            .get_l1_batch_pubdata_params(batch.header.number)
            .await
            .unwrap();

        match pubdata_params {
            Some(p) => {
                let commitment_mode = L1BatchCommitmentMode::from(p.pubdata_type);

                if commitment_mode == L1BatchCommitmentMode::Rollup
                    && self.pubdata_da == PubdataSendingMode::Custom
                {
                    if storage
                        .eth_sender_dal()
                        .is_using_blobs_in_latest_batch()
                        .await
                        .unwrap_or_default()
                    {
                        tracing::warn!("Overriding pubdata sending mode to Blobs, most likely rollup -> validium migration is in place");
                        (PubdataSendingMode::Blobs, commitment_mode)
                    } else {
                        tracing::warn!("Overriding pubdata sending mode to Calldata, most likely rollup -> validium migration is in place");
                        (PubdataSendingMode::Calldata, commitment_mode)
                    }
                } else if commitment_mode == L1BatchCommitmentMode::Validium
                    && self.pubdata_da != PubdataSendingMode::Custom
                {
                    tracing::warn!("Overriding pubdata sending mode to Custom, most likely validium -> rollup migration is in place");
                    (PubdataSendingMode::Custom, commitment_mode)
                } else {
                    (self.pubdata_da, self.commitment_mode)
                }
            }
            None => (self.pubdata_da, self.commitment_mode),
        }
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
            self.settlement_layer.is_gateway(),
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
}

async fn extract_ready_subrange(
    storage: &mut Connection<'_, Core>,
    publish_criteria: &mut [Box<dyn L1BatchPublishCriterion>],
    unpublished_l1_batches: Vec<L1BatchWithMetadata>,
    last_sealed_l1_batch: L1BatchNumber,
    is_gateway: bool,
) -> Option<Vec<L1BatchWithMetadata>> {
    let mut last_l1_batch: Option<L1BatchNumber> = None;
    for criterion in publish_criteria {
        let l1_batch_by_criterion = criterion
            .last_l1_batch_to_publish(
                storage,
                &unpublished_l1_batches,
                last_sealed_l1_batch,
                is_gateway,
            )
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
        match blob_store
            .get::<L1BatchProofForL1>(L1BatchProofForL1Key::Core((l1_batch_number, *version)))
            .await
        {
            Ok(proof) => return Some(proof),
            Err(ObjectStoreError::KeyNotFound(_)) => {
                match blob_store
                    .get::<L1BatchProofForL1<Bincode>>(L1BatchProofForL1Key::Core((
                        l1_batch_number,
                        *version,
                    )))
                    .await
                {
                    Ok(proof) => return Some(proof.into()),
                    Err(ObjectStoreError::KeyNotFound(_)) => continue, // proof is not ready yet, continue
                    Err(err) => panic!(
                        "Failed to load proof for batch {}: {}",
                        l1_batch_number.0, err
                    ),
                }
            }
            Err(err) => panic!(
                "Failed to load proof for batch {}: {}",
                l1_batch_number.0, err
            ),
        }
    }

    None
}
