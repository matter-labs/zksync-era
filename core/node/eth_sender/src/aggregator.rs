use std::{sync::Arc, time::Duration};

use chrono::Utc;
use zksync_airbender_prover_interface::outputs::L1BatchAirbenderSnarkProofForL1;
use zksync_config::configs::eth_sender::{
    PrecommitParams, ProofSendingMode, ProverType, SenderConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{blocks_dal::TxForPrecommit, Connection, ConnectionPool, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_object_store::{ObjectStore, ObjectStoreError, StoredObject};
use zksync_prover_interface::outputs::{
    L1BatchProofForL1, L1BatchProofForL1Key, TypedL1BatchProofForL1,
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
        execution_delay: Duration,
        is_gateway: bool,
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
                is_gateway,
                execution_delay,
            )
            .await?,
        ) {
            Ok(Some(op))
        } else if let Some(op) =
            restrictions.filter_prove_op(if restrictions.prove_restriction.is_some() {
                None
            } else {
                self.get_proof_operation(storage, last_sealed_l1_batch_number, l1_verifier_config)
                    .await
            })
        {
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
        priority_tree_last_executed_index: usize,
    ) -> &mut MiniMerkleTree<L1Tx> {
        if self.priority_merkle_tree.is_none() {
            // We unwrap here since it is only invoked during initialization
            let mut connection = self.pool.connection_tagged("eth_sender").await.unwrap();

            // We unwrap here since it is only invoked only once during initialization
            let priority_op_hashes = connection
                .transactions_dal()
                .get_l1_transactions_hashes(
                    priority_tree_start_index,
                    priority_tree_last_executed_index,
                )
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
        if l1_batch_number.is_none()
            && !ready_to_create_precommit_operation(
                &first_tx,
                last_tx,
                precommit_params,
                Utc::now().timestamp(),
            )
        {
            return Ok(None);
        }

        tracing::info!(
            "Creating precommit operation for L1 batch {} with first tx {:?}, last tx {:?} and params {:?}",
            l1_batch_for_precommit,
            first_tx.l2block_number,
            last_tx.l2block_number,
            precommit_params,

        );

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
        is_gateway: bool,
        execution_delay: Duration,
    ) -> Result<Option<ExecuteBatches>, EthSenderError> {
        let mut max_l1_batch_timestamp_millis =
            Some(unix_timestamp_ms() - execution_delay.as_millis() as u64);

        // Add safety margin for L1 block inclusion delays
        // On L1 time is discrete and in worst case if you send time at X,
        // it will be included in the block with timestamp X - 12.
        // So the margin should be greater than 12 sec, 30 seconds is used.
        // Apply margin only if execution_delay > 0
        if execution_delay > Duration::ZERO {
            const SAFETY_MARGIN_MS: u64 = 30_000; // 30 seconds in milliseconds
            max_l1_batch_timestamp_millis = max_l1_batch_timestamp_millis
                .map(|timestamp| timestamp.saturating_sub(SAFETY_MARGIN_MS));
        }
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
            is_gateway,
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
                logs: vec![],
                messages: vec![],
                message_roots: vec![],
            }));
        };

        // Initialize the priority ops merkle tree at the point it was left after the last executed batch.
        // If it was the very first batch we use the provided start index and fill the tree from scratch.
        let let_last_executed_priorty_op_id = storage
            .blocks_dal()
            .get_last_executed_priority_op_id()
            .await
            .unwrap()
            .unwrap_or(priority_tree_start_index);

        let priority_merkle_tree = self
            .get_or_init_tree(priority_tree_start_index, let_last_executed_priorty_op_id)
            .await;

        let mut priority_ops_proofs = vec![];
        let mut all_logs = vec![];
        let mut all_messages = vec![];
        let mut all_message_roots = vec![];
        for batch in &l1_batches {
            let priority_ops_in_batch = storage
                .blocks_dal()
                .get_batch_first_and_last_priority_op_id(batch.header.number)
                .await
                .unwrap()
                .filter(|(first_id, _last_id)| *first_id >= priority_tree_start_index);

            let count = batch.header.l1_tx_count as usize;

            // If there are priority operations in this batch. We have to prepare a merkle path for them.
            // For being always deterministic we build the tree that includes ONLY transcations up to the current batch.
            if let Some((first_priority_op_id_in_batch, last_priority_op_id_in_batch)) =
                priority_ops_in_batch
            {
                let new_l1_tx_hashes = storage
                    .transactions_dal()
                    .get_l1_transactions_hashes(
                        priority_tree_start_index + priority_merkle_tree.length(),
                        last_priority_op_id_in_batch,
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
            if is_gateway {
                let message_root = batch
                    .metadata
                    .aggregation_root
                    .ok_or(EthSenderError::MissingAggregationRoot(batch.header.number))?;
                all_logs.push(batch.header.l2_to_l1_logs.clone());
                all_messages.push(batch.header.l2_to_l1_messages.clone());
                all_message_roots.push(message_root);
            }
        }
        Ok(Some(ExecuteBatches {
            l1_batches,
            priority_ops_proofs,
            dependency_roots,
            logs: all_logs,
            messages: all_messages,
            message_roots: all_message_roots,
        }))
    }

    pub(super) fn set_prover(&mut self, prover: ProverType) {
        self.config.prover = prover;
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
        let mut ready_for_commit_l1_batches = if protocol_version_id.is_pre_boojum() {
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

        if protocol_version_id >= ProtocolVersionId::Version34 {
            ready_for_commit_l1_batches = ready_for_commit_l1_batches
                .into_iter()
                .take_while(|batch| {
                    let ready = batch
                        .metadata
                        .airbender
                        .as_ref()
                        .is_some_and(|a| !a.bootloader_initial_content_commitment.is_zero());
                    if !ready {
                        tracing::warn!(
                            l1_batch_number = %batch.header.number,
                            "Commit range truncated: missing Airbender bootloader commitment"
                        );
                    }
                    ready
                })
                .collect();
        }

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
                let commitment_mode = L1BatchCommitmentMode::from(p.pubdata_type());

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
        prover: ProverType,
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

        let proofs = Self::load_real_proofs(
            storage,
            l1_verifier_config,
            blob_store,
            batch_to_prove,
            minor_version,
            prover,
        )
        .await?;

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
            .get_l1_batch_metadata(batch_to_prove)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!("L1 batch #{batch_to_prove} with generated proof is not complete in the DB");
            });

        Some(ProveBatches {
            prev_l1_batch: previous_proven_batch_metadata,
            l1_batches: vec![metadata_for_batch_being_proved],
            proofs,
            should_verify: true,
        })
    }

    async fn load_real_proofs(
        storage: &mut Connection<'_, Core>,
        l1_verifier_config: L1VerifierConfig,
        blob_store: &dyn ObjectStore,
        batch: L1BatchNumber,
        minor_version: ProtocolVersionId,
        prover: ProverType,
    ) -> Option<Vec<L1BatchProofForL1>> {
        let fflonk_only = minor_version >= ProtocolVersionId::Version34;
        let prover = match prover {
            ProverType::Airbender if !fflonk_only => {
                tracing::warn!(
                    ?batch,
                    ?minor_version,
                    "Airbender L1 proofs require v34 or later"
                );
                return None;
            }
            ProverType::Both if !fflonk_only => ProverType::Boojum,
            prover => prover,
        };
        let mut patches = if prover.requires_boojum() {
            if fflonk_only {
                let Some(vk_hash) = l1_verifier_config.fflonk_snark_wrapper_vk_hash else {
                    tracing::warn!("The L1 verifier does not expose a FFLONK SNARK-wrapper VK");
                    return None;
                };
                storage
                    .protocol_versions_dal()
                    .get_patch_versions_for_fflonk_vk(minor_version, vk_hash)
                    .await
                    .unwrap()
            } else {
                storage
                    .protocol_versions_dal()
                    .get_patch_versions_for_vk(
                        minor_version,
                        l1_verifier_config.snark_wrapper_vk_hash,
                    )
                    .await
                    .unwrap()
            }
        } else {
            vec![]
        };
        if prover.requires_airbender() {
            let Some(vk_hash) = l1_verifier_config.airbender_snark_wrapper_vk_hash else {
                tracing::warn!("The L1 verifier does not expose an Airbender SNARK-wrapper VK");
                return None;
            };
            let airbender_patches = storage
                .protocol_versions_dal()
                .get_patch_versions_for_airbender_vk(minor_version, vk_hash)
                .await
                .unwrap();
            if prover.requires_boojum() {
                patches.retain(|patch| airbender_patches.contains(patch));
            } else {
                patches = airbender_patches;
            }
        }
        if patches.is_empty() {
            tracing::warn!(
                "No patch version of {minor_version:?} matches the required {prover:?} L1 VKs: {l1_verifier_config:?}"
            );
            return None;
        }

        for patch in patches {
            let version = ProtocolSemanticVersion {
                minor: minor_version,
                patch,
            };
            let mut proofs = Vec::with_capacity(2);
            if prover.requires_boojum() {
                let Some(proof) =
                    load_wrapped_fri_proofs_for_range(batch, blob_store, &[version]).await
                else {
                    continue;
                };
                if fflonk_only && !matches!(proof.inner(), TypedL1BatchProofForL1::Fflonk(_)) {
                    tracing::warn!("Boojum proof must be FFLONK for batch {batch}");
                    continue;
                }
                proofs.push(proof);
            }
            if prover.requires_airbender() {
                let Some(proof) = load_airbender_snark_proof(batch, blob_store, &[version]).await
                else {
                    continue;
                };
                proofs.push(proof);
            }
            if proofs
                .iter()
                .all(|proof| proof.protocol_version() == version)
            {
                return Some(proofs);
            }
            tracing::warn!("Proof protocol version does not match {version} for batch {batch}");
        }
        None
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

    /// Dummy proofs don't depend on the verification key, so they are available for any version.
    pub(crate) async fn has_proof_for_version(
        &self,
        l1_batch_number: L1BatchNumber,
        version: ProtocolSemanticVersion,
    ) -> bool {
        if self.config.proof_sending_mode == ProofSendingMode::SkipEveryProof {
            return true;
        }
        let prover = match self.config.prover {
            ProverType::Both if version.minor < ProtocolVersionId::Version34 => ProverType::Boojum,
            prover => prover,
        };
        if prover.requires_boojum()
            && load_wrapped_fri_proofs_for_range(l1_batch_number, &*self.blob_store, &[version])
                .await
                .is_none()
        {
            return false;
        }
        !prover.requires_airbender()
            || load_airbender_snark_proof(l1_batch_number, &*self.blob_store, &[version])
                .await
                .is_some()
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
                    self.config.prover,
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
                    self.config.prover,
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
            Err(ObjectStoreError::KeyNotFound(_)) => continue,
            Err(err) => panic!(
                "Failed to load proof for batch {}: {}",
                l1_batch_number.0, err
            ),
        }
    }

    None
}

fn ready_to_create_precommit_operation(
    first_tx: &TxForPrecommit,
    last_tx: &TxForPrecommit,
    precommit_params: &PrecommitParams,
    current_timestamp: i64,
) -> bool {
    let first_l2_block_age = current_timestamp - first_tx.timestamp;

    first_l2_block_age >= precommit_params.deadline.as_secs() as i64
        || last_tx.l2block_number.0 - first_tx.l2block_number.0
            >= precommit_params.l2_blocks_to_aggregate
}

/// Loads the SNARK-wrapped Airbender proof for `l1_batch_number` and decodes it.
async fn load_airbender_snark_proof(
    l1_batch_number: L1BatchNumber,
    blob_store: &dyn ObjectStore,
    allowed_versions: &[ProtocolSemanticVersion],
) -> Option<L1BatchProofForL1> {
    for version in allowed_versions {
        let snark_proof: L1BatchAirbenderSnarkProofForL1 =
            match blob_store.get((l1_batch_number, *version)).await {
                Ok(proof) => proof,
                Err(ObjectStoreError::KeyNotFound(_)) => continue,
                Err(err) => panic!(
                    "Failed to load Airbender SNARK proof for batch {}: {}",
                    l1_batch_number.0, err
                ),
            };

        // The SNARK prover stores a CBOR-encoded `L1BatchProofForL1` (plonk/fflonk), so it can be
        // submitted through the same path as Boojum proofs.
        let proof = <L1BatchProofForL1 as StoredObject>::deserialize(snark_proof.snark_proof)
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to deserialize Airbender SNARK proof for batch {}: {}",
                    l1_batch_number.0, err
                )
            });
        return Some(proof);
    }

    None
}

#[cfg(test)]
mod tests {
    use zksync_types::L2BlockNumber;

    use super::*;

    #[test]
    fn test_ready_to_create_precommit_operation() {
        let current_time = Utc::now().timestamp();
        let precommit_params = PrecommitParams {
            deadline: Duration::from_secs(60),
            l2_blocks_to_aggregate: 5,
        };

        // No precommit operation should be created if the first transaction is too fresh
        let first_tx = TxForPrecommit {
            l2block_number: L2BlockNumber(1),
            timestamp: current_time - 30,
            ..Default::default()
        };
        let last_tx = TxForPrecommit {
            l2block_number: L2BlockNumber(3),
            timestamp: current_time - 20,
            ..Default::default()
        };

        assert!(!ready_to_create_precommit_operation(
            &first_tx,
            &last_tx,
            &precommit_params,
            current_time
        ));

        // Tx too old
        let first_tx = TxForPrecommit {
            l1_batch_number: None,
            l2block_number: L2BlockNumber(1),
            timestamp: current_time - 70,
            tx_hash: Default::default(),
            is_success: false,
        };
        let last_tx = TxForPrecommit {
            l1_batch_number: None,
            l2block_number: L2BlockNumber(2),
            timestamp: current_time - 5,
            tx_hash: Default::default(),
            is_success: false,
        };

        assert!(ready_to_create_precommit_operation(
            &first_tx,
            &last_tx,
            &precommit_params,
            current_time
        ));

        let first_tx = TxForPrecommit {
            l1_batch_number: None,
            l2block_number: L2BlockNumber(1),
            timestamp: current_time - 70,
            tx_hash: Default::default(),
            is_success: false,
        };
        let last_tx = TxForPrecommit {
            l1_batch_number: None,
            l2block_number: L2BlockNumber(6),
            timestamp: current_time - 50,
            tx_hash: Default::default(),
            is_success: false,
        };

        assert!(ready_to_create_precommit_operation(
            &first_tx,
            &last_tx,
            &precommit_params,
            current_time
        ));
    }

    mod airbender {
        use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;
        use zksync_node_test_utils::{create_l1_batch, create_l1_batch_metadata};
        use zksync_object_store::MockObjectStore;
        use zksync_prover_interface::outputs::{
            AirbenderL1BatchProofForL1, TypedL1BatchProofForL1,
        };
        use zksync_types::{
            ethabi::{decode, ParamType, Token},
            protocol_upgrade::ProtocolVersion,
            H256, U256,
        };

        use super::*;

        fn test_version() -> ProtocolSemanticVersion {
            ProtocolSemanticVersion {
                minor: ProtocolVersionId::Version34,
                patch: 0.into(),
            }
        }

        fn batch_with_metadata(number: u32) -> L1BatchWithMetadata {
            let mut header = create_l1_batch(number);
            header.protocol_version = Some(ProtocolVersionId::Version34);
            let mut metadata = create_l1_batch_metadata(number);
            metadata.airbender = Some(zksync_types::commitment::AirbenderBatchCommitment {
                commitment: H256::repeat_byte(10),
                aux_data_hash: H256::repeat_byte(11),
                bootloader_initial_content_commitment: H256::repeat_byte(12),
                events_queue_commitment: H256::zero(),
            });
            L1BatchWithMetadata {
                header,
                metadata,
                raw_published_factory_deps: vec![],
            }
        }

        /// A SNARK blob whose `snark_proof` bytes are a CBOR-encoded `L1BatchProofForL1`, exactly
        /// what the proof data handler stores and `load_airbender_snark_proof` expects to decode.
        fn snark_blob(version: ProtocolSemanticVersion) -> L1BatchAirbenderSnarkProofForL1 {
            let proof = L1BatchProofForL1::new_airbender(AirbenderL1BatchProofForL1 {
                proof: vec![0x11; 44 * 32],
                protocol_version: version,
            });
            let snark_proof = <L1BatchProofForL1 as StoredObject>::serialize(&proof).unwrap();
            L1BatchAirbenderSnarkProofForL1 { snark_proof }
        }

        fn boojum_proof(version: ProtocolSemanticVersion) -> L1BatchProofForL1 {
            let proof = L1BatchProofForL1::deserialize(
                include_bytes!("../../../lib/prover_interface/tests/l1_batch_proof_1_0_27_0.cbor")
                    .to_vec(),
            )
            .unwrap();
            match proof.inner() {
                TypedL1BatchProofForL1::Fflonk(mut proof) => {
                    proof.protocol_version = version;
                    L1BatchProofForL1::new_fflonk(proof)
                }
                TypedL1BatchProofForL1::Plonk(mut proof) => {
                    proof.protocol_version = version;
                    L1BatchProofForL1::new_plonk(proof)
                }
                TypedL1BatchProofForL1::Airbender(_) => panic!("expected Boojum fixture"),
            }
        }

        #[test_casing::test_casing(4, [(false, false), (true, false), (false, true), (true, true)])]
        #[tokio::test]
        async fn proof_selection_waits_for_required_proofs(
            boojum_ready: bool,
            airbender_ready: bool,
        ) {
            let pool = ConnectionPool::<Core>::test_pool().await;
            let mut storage = pool.connection().await.unwrap();
            let blob_store = MockObjectStore::arc();
            let batch = L1BatchNumber(1);
            let boojum_version = test_version();
            let airbender_version = boojum_version;
            let verifier = L1VerifierConfig {
                snark_wrapper_vk_hash: H256::repeat_byte(1),
                fflonk_snark_wrapper_vk_hash: Some(H256::repeat_byte(5)),
                airbender_snark_wrapper_vk_hash: Some(H256::repeat_byte(2)),
            };
            storage
                .protocol_versions_dal()
                .save_protocol_version_with_tx(&ProtocolVersion {
                    version: boojum_version,
                    l1_verifier_config: verifier,
                    ..Default::default()
                })
                .await
                .unwrap();
            blob_store
                .put(
                    L1BatchProofForL1Key::Core((
                        if boojum_ready { batch } else { batch + 1 },
                        boojum_version,
                    )),
                    &boojum_proof(boojum_version),
                )
                .await
                .unwrap();
            blob_store
                .put(
                    (
                        if airbender_ready { batch } else { batch + 1 },
                        airbender_version,
                    ),
                    &snark_blob(airbender_version),
                )
                .await
                .unwrap();

            for (prover, ready, versions) in [
                (ProverType::Boojum, boojum_ready, vec![boojum_version]),
                (
                    ProverType::Airbender,
                    airbender_ready,
                    vec![airbender_version],
                ),
                (
                    ProverType::Both,
                    boojum_ready && airbender_ready,
                    vec![boojum_version, airbender_version],
                ),
            ] {
                let proofs = Aggregator::load_real_proofs(
                    &mut storage,
                    L1VerifierConfig {
                        snark_wrapper_vk_hash: verifier.fflonk_snark_wrapper_vk_hash.unwrap(),
                        ..verifier
                    },
                    &*blob_store,
                    batch,
                    boojum_version.minor,
                    prover,
                )
                .await;
                assert_eq!(proofs.is_some(), ready, "{prover:?}");
                if let Some(proofs) = proofs {
                    assert_eq!(
                        proofs
                            .iter()
                            .map(L1BatchProofForL1::protocol_version)
                            .collect::<Vec<_>>(),
                        versions
                    );
                }
            }
            for fflonk_vk in [None, Some(H256::repeat_byte(4))] {
                let keys = L1VerifierConfig {
                    fflonk_snark_wrapper_vk_hash: fflonk_vk,
                    ..verifier
                };
                assert!(Aggregator::load_real_proofs(
                    &mut storage,
                    keys,
                    &*blob_store,
                    batch,
                    boojum_version.minor,
                    ProverType::Both,
                )
                .await
                .is_none());
            }
            for airbender_vk in [None, Some(H256::repeat_byte(4))] {
                let keys = L1VerifierConfig {
                    airbender_snark_wrapper_vk_hash: airbender_vk,
                    ..verifier
                };
                assert!(Aggregator::load_real_proofs(
                    &mut storage,
                    keys,
                    &*blob_store,
                    batch,
                    boojum_version.minor,
                    ProverType::Both,
                )
                .await
                .is_none());
            }
        }

        #[test_casing::test_casing(4, [(false, false), (true, false), (false, true), (true, true)])]
        #[tokio::test]
        async fn proof_selection_requires_matching_patches(shared_keys: bool, boojum_newer: bool) {
            let pool = ConnectionPool::<Core>::test_pool().await;
            let mut storage = pool.connection().await.unwrap();
            let blob_store = MockObjectStore::arc();
            let batch = L1BatchNumber(1);
            let older = test_version();
            let newer = ProtocolSemanticVersion {
                patch: 1.into(),
                ..older
            };
            let (boojum_version, airbender_version) = if boojum_newer {
                (newer, older)
            } else {
                (older, newer)
            };
            let verifier = L1VerifierConfig {
                snark_wrapper_vk_hash: H256::repeat_byte(1),
                fflonk_snark_wrapper_vk_hash: Some(H256::repeat_byte(5)),
                airbender_snark_wrapper_vk_hash: Some(H256::repeat_byte(2)),
            };
            for version in [older, newer] {
                let mut keys = verifier;
                if !shared_keys {
                    if version != boojum_version {
                        keys.fflonk_snark_wrapper_vk_hash = Some(H256::repeat_byte(3));
                    }
                    if version != airbender_version {
                        keys.airbender_snark_wrapper_vk_hash = Some(H256::repeat_byte(4));
                    }
                }
                storage
                    .protocol_versions_dal()
                    .save_protocol_version_with_tx(&ProtocolVersion {
                        version,
                        l1_verifier_config: keys,
                        ..Default::default()
                    })
                    .await
                    .unwrap();
            }
            blob_store
                .put(
                    L1BatchProofForL1Key::Core((batch, boojum_version)),
                    &boojum_proof(boojum_version),
                )
                .await
                .unwrap();
            blob_store
                .put((batch, airbender_version), &snark_blob(airbender_version))
                .await
                .unwrap();

            for (prover, version) in [
                (ProverType::Boojum, boojum_version),
                (ProverType::Airbender, airbender_version),
            ] {
                let proofs = Aggregator::load_real_proofs(
                    &mut storage,
                    verifier,
                    &*blob_store,
                    batch,
                    older.minor,
                    prover,
                )
                .await
                .unwrap();
                assert_eq!(proofs.len(), 1);
                assert_eq!(proofs[0].protocol_version(), version);
            }
            assert!(Aggregator::load_real_proofs(
                &mut storage,
                verifier,
                &*blob_store,
                batch,
                older.minor,
                ProverType::Both,
            )
            .await
            .is_none());

            for version in [older, newer] {
                blob_store
                    .put(
                        L1BatchProofForL1Key::Core((batch, version)),
                        &boojum_proof(version),
                    )
                    .await
                    .unwrap();
                blob_store
                    .put((batch, version), &snark_blob(version))
                    .await
                    .unwrap();
                let proofs = Aggregator::load_real_proofs(
                    &mut storage,
                    verifier,
                    &*blob_store,
                    batch,
                    older.minor,
                    ProverType::Both,
                )
                .await;
                assert_eq!(proofs.is_some(), shared_keys);
                if let Some(proofs) = proofs {
                    assert_eq!(proofs.len(), 2);
                    assert!(proofs
                        .iter()
                        .all(|proof| proof.protocol_version() == version));
                }
            }
        }

        #[test_casing::test_casing(2, [false, true])]
        #[tokio::test]
        async fn proof_selection_rejects_mismatched_blob_version(boojum_mismatch: bool) {
            let pool = ConnectionPool::<Core>::test_pool().await;
            let mut storage = pool.connection().await.unwrap();
            let blob_store = MockObjectStore::arc();
            let batch = L1BatchNumber(1);
            let version = test_version();
            let other_version = ProtocolSemanticVersion {
                patch: 1.into(),
                ..version
            };
            let verifier = L1VerifierConfig {
                snark_wrapper_vk_hash: H256::repeat_byte(1),
                fflonk_snark_wrapper_vk_hash: Some(H256::repeat_byte(5)),
                airbender_snark_wrapper_vk_hash: Some(H256::repeat_byte(2)),
            };
            storage
                .protocol_versions_dal()
                .save_protocol_version_with_tx(&ProtocolVersion {
                    version,
                    l1_verifier_config: verifier,
                    ..Default::default()
                })
                .await
                .unwrap();
            blob_store
                .put(
                    L1BatchProofForL1Key::Core((batch, version)),
                    &boojum_proof(if boojum_mismatch {
                        other_version
                    } else {
                        version
                    }),
                )
                .await
                .unwrap();
            blob_store
                .put(
                    (batch, version),
                    &snark_blob(if boojum_mismatch {
                        version
                    } else {
                        other_version
                    }),
                )
                .await
                .unwrap();
            assert!(Aggregator::load_real_proofs(
                &mut storage,
                verifier,
                &*blob_store,
                batch,
                version.minor,
                ProverType::Both,
            )
            .await
            .is_none());
        }

        #[test]
        fn v34_proof_envelopes() {
            let version = test_version();
            let boojum = boojum_proof(version);
            let airbender =
                L1BatchProofForL1::deserialize(snark_blob(version).snark_proof).unwrap();
            let proof_array = ParamType::Array(Box::new(ParamType::Uint(256)));
            let stored = StoredBatchInfo::schema_for_protocol_version(version.minor);
            let decode_proof = |proofs| {
                let op = ProveBatches {
                    prev_l1_batch: batch_with_metadata(0),
                    l1_batches: vec![batch_with_metadata(1)],
                    proofs,
                    should_verify: true,
                };
                let args = op.conditional_into_tokens(false);
                assert_eq!(&args[..2], &[Token::Uint(1.into()), Token::Uint(1.into())]);
                let Token::Bytes(data) = &args[2] else {
                    panic!("expected proof bytes")
                };
                assert_eq!(data[0], 5);
                let decoded = decode(
                    &[
                        stored.clone(),
                        ParamType::Array(Box::new(stored.clone())),
                        proof_array.clone(),
                    ],
                    &data[1..],
                )
                .unwrap();
                let Token::Array(proof) = &decoded[2] else {
                    panic!("expected proof envelope")
                };
                proof.clone()
            };
            let combined = decode_proof(vec![boojum.clone(), airbender.clone()]);
            assert_eq!(combined[0], Token::Uint(4.into()));
            assert_eq!(combined.len(), 1 + 24 + 44);
            assert_eq!(
                &combined[25..],
                vec![Token::Uint(U256::from_big_endian(&[0x11; 32])); 44]
            );
            let boojum_only = decode_proof(vec![boojum]);
            assert_eq!(&boojum_only[..25], &combined[..25]);
            assert_eq!(&boojum_only[25..], vec![Token::Uint(U256::zero()); 44]);
            let airbender_only = decode_proof(vec![airbender]);
            assert_eq!(airbender_only.len(), combined.len());
            assert_eq!(airbender_only[0], Token::Uint(4.into()));
            assert_eq!(&airbender_only[1..25], vec![Token::Uint(U256::zero()); 24]);
            assert_eq!(&airbender_only[25..], &combined[25..]);
        }

        #[test]
        #[should_panic]
        fn malformed_airbender_proof_is_rejected() {
            ProveBatches {
                prev_l1_batch: batch_with_metadata(0),
                l1_batches: vec![batch_with_metadata(1)],
                proofs: vec![L1BatchProofForL1::new_airbender(
                    AirbenderL1BatchProofForL1 {
                        proof: vec![0; 43 * 32],
                        protocol_version: test_version(),
                    },
                )],
                should_verify: true,
            }
            .conditional_into_tokens(false);
        }

        #[test]
        fn v34_commit_execute_and_precommit_encoding() {
            use zksync_l1_contract_interface::{
                i_executor::{
                    methods::{CommitBatches, PrecommitBatches},
                    structures::CommitBatchInfo,
                },
                Tokenizable, Tokenize,
            };
            use zksync_types::{commitment::AirbenderBatchCommitment, Address};
            let mut batch = batch_with_metadata(1);
            batch.metadata.airbender = Some(AirbenderBatchCommitment {
                commitment: H256::repeat_byte(10),
                aux_data_hash: H256::repeat_byte(11),
                bootloader_initial_content_commitment: H256::repeat_byte(12),
                events_queue_commitment: H256::zero(),
            });
            let mut previous = batch_with_metadata(0);
            previous.header.protocol_version = Some(ProtocolVersionId::Version33);
            {
                let batches = vec![batch.clone()];
                let args = (&CommitBatches {
                    last_committed_l1_batch: &previous,
                    l1_batches: &batches,
                    mode: L1BatchCommitmentMode::Validium,
                    pubdata_da: PubdataSendingMode::Calldata,
                })
                    .into_tokens();
                let Token::Bytes(data) = &args[2] else {
                    panic!("commit bytes")
                };
                assert_eq!(data[0], 5);
                let stored =
                    StoredBatchInfo::schema_for_protocol_version(ProtocolVersionId::Version34);
                let decoded = decode(
                    &[
                        stored.clone(),
                        ParamType::Array(Box::new(CommitBatchInfo::schema_for_protocol_version(
                            ProtocolVersionId::Version34,
                        ))),
                    ],
                    &data[1..],
                )
                .unwrap();
                let Token::Tuple(prev) = &decoded[0] else {
                    panic!("previous batch")
                };
                assert_eq!(prev.len(), 10);
                assert_eq!(prev[9], Token::FixedBytes(vec![0; 32]));
                let Token::Array(commits) = &decoded[1] else {
                    panic!("commit array")
                };
                let Token::Tuple(fields) = &commits[0] else {
                    panic!("commit fields")
                };
                assert_eq!(fields.len(), 11);
                assert_eq!(fields[8], Token::FixedBytes(vec![12; 32]));
                assert_eq!(
                    fields[6],
                    Token::FixedBytes(
                        batch
                            .metadata
                            .bootloader_initial_content_commitment
                            .unwrap()
                            .as_bytes()
                            .to_vec()
                    )
                );
                assert_eq!(
                    fields[7],
                    Token::FixedBytes(
                        batch
                            .metadata
                            .events_queue_commitment
                            .unwrap()
                            .as_bytes()
                            .to_vec()
                    )
                );
                let args = ExecuteBatches {
                    l1_batches: batches,
                    priority_ops_proofs: vec![],
                    dependency_roots: vec![],
                    logs: vec![],
                    messages: vec![],
                    message_roots: vec![],
                }
                .encode_for_eth_tx(ProtocolVersionId::Version34, Address::zero());
                let Token::Bytes(data) = &args[2] else {
                    panic!("execute bytes")
                };
                assert_eq!(data[0], 5);
                let decoded = decode(&[ParamType::Array(Box::new(stored))], &data[1..]).unwrap();
                assert_eq!(
                    decoded[0],
                    Token::Array(vec![StoredBatchInfo::from(&batch)
                        .into_token_with_protocol_version(ProtocolVersionId::Version34)])
                );
            }
            let args = (&PrecommitBatches {
                txs: &[],
                last_l2_block: L2BlockNumber(1),
                l1_batch_number: L1BatchNumber(1),
            })
                .into_tokens();
            let Token::Bytes(data) = &args[1] else {
                panic!("precommit bytes")
            };
            assert_eq!(data[0], 1);
            let legacy = CommitBatchInfo::new(
                L1BatchCommitmentMode::Validium,
                &previous,
                PubdataSendingMode::Calldata,
            )
            .into_token();
            assert_eq!(legacy.into_tuple().unwrap().len(), 10);
        }

        #[test_casing::test_casing(4, [
            (ProtocolVersionId::Version33, L1BatchNumber(0)),
            (ProtocolVersionId::Version33, L1BatchNumber(1)),
            (ProtocolVersionId::Version34, L1BatchNumber(0)),
            (ProtocolVersionId::Version34, L1BatchNumber(1)),
        ])]
        #[tokio::test]
        async fn commitments_and_predecessor_follow_l1(
            version: ProtocolVersionId,
            number: L1BatchNumber,
        ) {
            use zksync_types::{
                block::L1BatchTreeData,
                commitment::{AirbenderBatchCommitment, L1BatchCommitmentArtifacts},
            };
            let pool = ConnectionPool::<Core>::test_pool().await;
            let mut storage = pool.connection().await.unwrap();
            storage
                .protocol_versions_dal()
                .save_protocol_version_with_tx(&ProtocolVersion {
                    version: ProtocolSemanticVersion {
                        minor: version,
                        ..test_version()
                    },
                    ..Default::default()
                })
                .await
                .unwrap();
            let mut header = batch_with_metadata(number.0).header;
            header.protocol_version = Some(version);
            storage
                .blocks_dal()
                .insert_mock_l1_batch(&header)
                .await
                .unwrap();
            storage
                .blocks_dal()
                .save_l1_batch_tree_data(
                    number,
                    &L1BatchTreeData {
                        hash: H256::repeat_byte(3),
                        rollup_last_leaf_index: 1,
                    },
                )
                .await
                .unwrap();
            let mut artifacts = L1BatchCommitmentArtifacts::default();
            artifacts.commitment_hash.commitment = H256::repeat_byte(4);
            artifacts.commitment_hash.aux_output = H256::repeat_byte(5);
            storage
                .blocks_dal()
                .save_l1_batch_commitment_artifacts(number, &artifacts)
                .await
                .unwrap();
            let boojum = storage
                .blocks_dal()
                .get_l1_batch_metadata(number)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(boojum.metadata.airbender, None);
            let predecessor = storage
                .blocks_dal()
                .get_prev_batch_airbender_commitment_input(number, ProtocolVersionId::Version34)
                .await
                .unwrap();
            if version >= ProtocolVersionId::Version34 {
                assert!(predecessor.is_none());
            } else {
                assert_eq!(
                    predecessor.unwrap().prev_batch_commitment,
                    boojum.metadata.commitment
                );
            }
            let airbender = AirbenderBatchCommitment {
                commitment: H256::repeat_byte(10),
                aux_data_hash: H256::repeat_byte(11),
                bootloader_initial_content_commitment: H256::repeat_byte(12),
                events_queue_commitment: H256::zero(),
            };
            storage
                .blocks_dal()
                .save_airbender_batch_commitment(number, &airbender)
                .await
                .unwrap();
            assert_eq!(
                storage
                    .blocks_dal()
                    .get_airbender_batch_commitment(number)
                    .await
                    .unwrap(),
                Some(airbender)
            );
            let batch = storage
                .blocks_dal()
                .get_l1_batch_metadata(number)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(batch.metadata.airbender, Some(airbender));
            assert_eq!(batch.metadata.commitment, boojum.metadata.commitment);
            assert_eq!(batch.metadata.aux_data_hash, boojum.metadata.aux_data_hash);
            assert_eq!(
                batch.metadata.events_queue_commitment,
                boojum.metadata.events_queue_commitment
            );
            assert_eq!(
                batch.metadata.bootloader_initial_content_commitment,
                boojum.metadata.bootloader_initial_content_commitment
            );
            let stored = StoredBatchInfo::from(&batch);
            assert_eq!(stored.commitment, H256::repeat_byte(4));
            assert_eq!(
                stored.airbender_commitment,
                if version >= ProtocolVersionId::Version34 {
                    airbender.commitment
                } else {
                    H256::zero()
                }
            );
            let prev = storage
                .blocks_dal()
                .get_prev_batch_airbender_commitment_input(number, ProtocolVersionId::Version34)
                .await
                .unwrap()
                .unwrap();
            if version >= ProtocolVersionId::Version34 {
                assert_eq!(prev.prev_batch_commitment, airbender.commitment);
                assert_eq!(prev.prev_aux_hash, airbender.aux_data_hash);
            } else {
                assert_eq!(prev.prev_batch_commitment, boojum.metadata.commitment);
                assert_eq!(prev.prev_aux_hash, boojum.metadata.aux_data_hash);
            }
            let legacy = storage
                .blocks_dal()
                .get_prev_batch_airbender_commitment_input(number, ProtocolVersionId::Version33)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(legacy.prev_batch_commitment, airbender.commitment);
            assert_eq!(legacy.prev_aux_hash, airbender.aux_data_hash);
        }

        #[test_casing::test_casing(2, [ProtocolVersionId::Version33, ProtocolVersionId::Version34])]
        #[tokio::test]
        async fn proof_submission_respects_protocol_version(current_version: ProtocolVersionId) {
            use zksync_types::{
                aggregated_operations::AggregatedActionType, block::L1BatchTreeData,
                commitment::L1BatchCommitmentArtifacts, Address,
            };
            let pool = ConnectionPool::<Core>::test_pool().await;
            let mut storage = pool.connection().await.unwrap();
            let keys = L1VerifierConfig {
                snark_wrapper_vk_hash: H256::repeat_byte(1),
                fflonk_snark_wrapper_vk_hash: Some(H256::repeat_byte(5)),
                airbender_snark_wrapper_vk_hash: Some(H256::repeat_byte(2)),
            };
            for (number, version, action) in [
                (
                    1,
                    ProtocolVersionId::Version33,
                    L1BatchAggregatedActionType::PublishProofOnchain,
                ),
                (2, current_version, L1BatchAggregatedActionType::Commit),
            ] {
                storage
                    .protocol_versions_dal()
                    .save_protocol_version_with_tx(&ProtocolVersion {
                        version: ProtocolSemanticVersion {
                            minor: version,
                            patch: 0.into(),
                        },
                        l1_verifier_config: keys,
                        ..Default::default()
                    })
                    .await
                    .unwrap();
                let mut batch = batch_with_metadata(number);
                batch.header.protocol_version = Some(version);
                let number = batch.header.number;
                storage
                    .blocks_dal()
                    .insert_mock_l1_batch(&batch.header)
                    .await
                    .unwrap();
                storage
                    .blocks_dal()
                    .save_l1_batch_tree_data(
                        number,
                        &L1BatchTreeData {
                            hash: H256::repeat_byte(3),
                            rollup_last_leaf_index: 1,
                        },
                    )
                    .await
                    .unwrap();
                let mut artifacts = L1BatchCommitmentArtifacts::default();
                artifacts.commitment_hash.commitment = H256::repeat_byte(4);
                artifacts.commitment_hash.aux_output = H256::repeat_byte(5);
                storage
                    .blocks_dal()
                    .save_l1_batch_commitment_artifacts(number, &artifacts)
                    .await
                    .unwrap();
                storage
                    .blocks_dal()
                    .save_airbender_batch_commitment(number, &batch.metadata.airbender.unwrap())
                    .await
                    .unwrap();
                let action = AggregatedActionType::L1Batch(action);
                let tx = storage
                    .eth_sender_dal()
                    .save_eth_tx(0, vec![], action, Address::zero(), None, None, None, false)
                    .await
                    .unwrap();
                storage
                    .blocks_dal()
                    .set_eth_tx_id_for_l1_batches(number..=number, tx.id, action)
                    .await
                    .unwrap();
            }
            let version = ProtocolSemanticVersion {
                minor: current_version,
                patch: 0.into(),
            };
            let blobs = MockObjectStore::arc();
            blobs
                .put(
                    L1BatchProofForL1Key::Core((L1BatchNumber(2), version)),
                    &boojum_proof(version),
                )
                .await
                .unwrap();
            blobs
                .put((L1BatchNumber(2), version), &snark_blob(version))
                .await
                .unwrap();
            for prover in [ProverType::Both, ProverType::Airbender, ProverType::Boojum] {
                let operation = Aggregator::load_real_proof_operation(
                    &mut storage,
                    keys,
                    &*blobs,
                    false,
                    prover,
                )
                .await;
                if current_version < ProtocolVersionId::Version34 && prover == ProverType::Airbender
                {
                    assert!(operation.is_none());
                    continue;
                }
                let operation = operation.unwrap();
                assert_eq!(
                    operation.prev_l1_batch.metadata.commitment,
                    H256::repeat_byte(4)
                );
                assert_eq!(
                    operation.proofs.len(),
                    if prover == ProverType::Both && current_version >= ProtocolVersionId::Version34
                    {
                        2
                    } else {
                        1
                    }
                );
                let stored = StoredBatchInfo::from(&operation.l1_batches[0]);
                assert_eq!(stored.commitment, H256::repeat_byte(4));
                assert_eq!(
                    stored.airbender_commitment,
                    if current_version >= ProtocolVersionId::Version34 {
                        H256::repeat_byte(10)
                    } else {
                        H256::zero()
                    }
                );
            }
        }

        // Prove path: the SNARK proof stored under `(batch, version)` is loaded and decoded when
        // its version is allowed by L1.
        #[tokio::test]
        async fn load_airbender_snark_proof_returns_versioned_proof() {
            let blob_store = MockObjectStore::arc();
            let batch = L1BatchNumber(1);
            let version = test_version();

            blob_store
                .put((batch, version), &snark_blob(version))
                .await
                .unwrap();

            let loaded = load_airbender_snark_proof(batch, &*blob_store, &[version]).await;

            let proof = loaded.expect("proof should be loaded");
            assert_eq!(proof.protocol_version(), version);
        }

        // No SNARK proof uploaded yet => nothing to prove.
        #[tokio::test]
        async fn load_airbender_snark_proof_returns_none_without_snark() {
            let blob_store = MockObjectStore::arc();
            let batch = L1BatchNumber(1);

            let loaded = load_airbender_snark_proof(batch, &*blob_store, &[test_version()]).await;
            assert!(loaded.is_none());
        }

        // The proof blob exists, but under a version that isn't in the L1-allowed set => not
        // loaded, mirroring the Boojum VK-disambiguation behaviour.
        #[tokio::test]
        async fn load_airbender_snark_proof_returns_none_for_disallowed_version() {
            let blob_store = MockObjectStore::arc();
            let batch = L1BatchNumber(1);
            let stored_version = test_version();
            let allowed_version = ProtocolSemanticVersion {
                minor: ProtocolVersionId::Version34,
                patch: 1.into(),
            };

            blob_store
                .put((batch, stored_version), &snark_blob(stored_version))
                .await
                .unwrap();

            let loaded = load_airbender_snark_proof(batch, &*blob_store, &[allowed_version]).await;
            assert!(loaded.is_none());
        }
    }
}
