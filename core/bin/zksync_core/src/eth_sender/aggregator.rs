use zksync_config::configs::eth_sender::{ProofSendingMode, SenderConfig};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::StorageProcessor;
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, AggregatedOperation, BlocksCommitOperation, BlocksExecuteOperation,
        BlocksProofOperation,
    },
    commitment::BlockWithMetadata,
    helpers::unix_timestamp_ms,
    L1BatchNumber,
};

use crate::eth_sender::block_publish_criterion::{
    BlockNumberCriterion, BlockPublishCriterion, DataSizeCriterion, GasCriterion,
    TimestampDeadlineCriterion,
};

#[derive(Debug)]
pub struct Aggregator {
    commit_criterion: Vec<Box<dyn BlockPublishCriterion>>,
    proof_criterion: Vec<Box<dyn BlockPublishCriterion>>,
    execute_criterion: Vec<Box<dyn BlockPublishCriterion>>,
    config: SenderConfig,
}

impl Aggregator {
    pub fn new(config: SenderConfig) -> Self {
        Self {
            commit_criterion: vec![
                Box::from(BlockNumberCriterion {
                    op: AggregatedActionType::CommitBlocks,
                    limit: config.max_aggregated_blocks_to_commit,
                }),
                Box::from(GasCriterion::new(
                    AggregatedActionType::CommitBlocks,
                    config.max_aggregated_tx_gas,
                )),
                Box::from(DataSizeCriterion {
                    op: AggregatedActionType::CommitBlocks,
                    data_limit: config.max_eth_tx_data_size,
                }),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::CommitBlocks,
                    deadline_seconds: config.aggregated_block_commit_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
            ],
            proof_criterion: vec![
                Box::from(BlockNumberCriterion {
                    op: AggregatedActionType::PublishProofBlocksOnchain,
                    limit: *config.aggregated_proof_sizes.iter().max().unwrap() as u32,
                }),
                Box::from(GasCriterion::new(
                    AggregatedActionType::PublishProofBlocksOnchain,
                    config.max_aggregated_tx_gas,
                )),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::PublishProofBlocksOnchain,
                    deadline_seconds: config.aggregated_block_prove_deadline,
                    // Currently, we can't use this functionality for proof criterion
                    // since we don't send dummy and real proofs in the same range,
                    // so even small ranges must be closed.
                    max_allowed_lag: None,
                }),
            ],
            execute_criterion: vec![
                Box::from(BlockNumberCriterion {
                    op: AggregatedActionType::ExecuteBlocks,
                    limit: config.max_aggregated_blocks_to_execute,
                }),
                Box::from(GasCriterion::new(
                    AggregatedActionType::ExecuteBlocks,
                    config.max_aggregated_tx_gas,
                )),
                Box::from(TimestampDeadlineCriterion {
                    op: AggregatedActionType::ExecuteBlocks,
                    deadline_seconds: config.aggregated_block_execute_deadline,
                    max_allowed_lag: Some(config.timestamp_criteria_max_allowed_lag),
                }),
            ],
            config,
        }
    }

    pub async fn get_next_ready_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        prover_storage: &mut StorageProcessor<'_>,
        base_system_contracts_hashes: BaseSystemContractsHashes,
    ) -> Option<AggregatedOperation> {
        let last_sealed_block_number = storage.blocks_dal().get_sealed_block_number().await;
        if let Some(op) = self
            .get_execute_operations(
                storage,
                self.config.max_aggregated_blocks_to_execute as usize,
                last_sealed_block_number,
            )
            .await
        {
            Some(AggregatedOperation::ExecuteBlocks(op))
        } else if let Some(op) = self
            .get_proof_operation(
                storage,
                prover_storage,
                *self.config.aggregated_proof_sizes.iter().max().unwrap(),
                last_sealed_block_number,
            )
            .await
        {
            Some(AggregatedOperation::PublishProofBlocksOnchain(op))
        } else {
            self.get_commit_operation(
                storage,
                self.config.max_aggregated_blocks_to_commit as usize,
                last_sealed_block_number,
                base_system_contracts_hashes,
            )
            .await
            .map(AggregatedOperation::CommitBlocks)
        }
    }

    async fn get_execute_operations(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        limit: usize,
        last_sealed_block: L1BatchNumber,
    ) -> Option<BlocksExecuteOperation> {
        let max_l1_batch_timestamp_millis = self
            .config
            .l1_batch_min_age_before_execute_seconds
            .map(|age| unix_timestamp_ms() - age * 1_000);
        let ready_for_execute_blocks = storage
            .blocks_dal()
            .get_ready_for_execute_blocks(limit, max_l1_batch_timestamp_millis)
            .await;
        let blocks = extract_ready_subrange(
            storage,
            &mut self.execute_criterion,
            ready_for_execute_blocks,
            last_sealed_block,
        )
        .await;

        blocks.map(|blocks| BlocksExecuteOperation { blocks })
    }

    async fn get_commit_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        limit: usize,
        last_sealed_block: L1BatchNumber,
        base_system_contracts_hashes: BaseSystemContractsHashes,
    ) -> Option<BlocksCommitOperation> {
        let mut blocks_dal = storage.blocks_dal();

        let last_block = blocks_dal.get_last_committed_to_eth_block().await?;

        let ready_for_commit_blocks = blocks_dal
            .get_ready_for_commit_blocks(
                limit,
                base_system_contracts_hashes.bootloader,
                base_system_contracts_hashes.default_aa,
            )
            .await;

        // Check that the blocks that are selected are sequential
        ready_for_commit_blocks
            .iter()
            .reduce(|last_block, next_block| {
                if last_block.header.number + 1 == next_block.header.number {
                    next_block
                } else {
                    panic!("Blocks are not sequential")
                }
            });

        let blocks = extract_ready_subrange(
            storage,
            &mut self.commit_criterion,
            ready_for_commit_blocks,
            last_sealed_block,
        )
        .await;
        blocks.map(|blocks| BlocksCommitOperation {
            last_committed_block: last_block,
            blocks,
        })
    }

    async fn load_real_proof_operation(
        storage: &mut StorageProcessor<'_>,
        prover_storage: &mut StorageProcessor<'_>,
    ) -> Option<BlocksProofOperation> {
        let previous_proven_block_number =
            storage.blocks_dal().get_last_l1_batch_with_prove_tx().await;
        let proofs = prover_storage
            .prover_dal()
            .get_final_proofs_for_blocks(
                previous_proven_block_number + 1,
                previous_proven_block_number + 1,
            )
            .await;
        if proofs.is_empty() {
            // The proof for the next block is not generated yet
            return None;
        }

        assert_eq!(proofs.len(), 1);

        let previous_proven_block_metadata = storage
            .blocks_dal()
            .get_block_metadata(previous_proven_block_number)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "Block number {} with submitted proof is not complete in the DB",
                    previous_proven_block_number
                )
            });
        let block_to_prove_metadata = storage
            .blocks_dal()
            .get_block_metadata(previous_proven_block_number + 1)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "Block number {} with generated proof is not complete in the DB",
                    previous_proven_block_number + 1
                )
            });

        Some(BlocksProofOperation {
            prev_block: previous_proven_block_metadata,
            blocks: vec![block_to_prove_metadata],
            proofs,
            should_verify: true,
        })
    }

    async fn prepare_dummy_proof_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        ready_for_proof_blocks: Vec<BlockWithMetadata>,
        last_sealed_block: L1BatchNumber,
    ) -> Option<BlocksProofOperation> {
        if let Some(blocks) = extract_ready_subrange(
            storage,
            &mut self.proof_criterion,
            ready_for_proof_blocks,
            last_sealed_block,
        )
        .await
        {
            let prev_block_number = blocks.first().map(|bl| bl.header.number - 1)?;
            let prev_block = storage
                .blocks_dal()
                .get_block_metadata(prev_block_number)
                .await?;

            Some(BlocksProofOperation {
                prev_block,
                blocks,
                proofs: vec![],
                should_verify: false,
            })
        } else {
            None
        }
    }

    async fn get_proof_operation(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        prover_storage: &mut StorageProcessor<'_>,
        limit: usize,
        last_sealed_block: L1BatchNumber,
    ) -> Option<BlocksProofOperation> {
        match self.config.proof_sending_mode {
            ProofSendingMode::OnlyRealProofs => {
                Self::load_real_proof_operation(storage, prover_storage).await
            }
            ProofSendingMode::SkipEveryProof => {
                let ready_for_proof_blocks = storage
                    .blocks_dal()
                    .get_ready_for_dummy_proof_blocks(limit)
                    .await;
                self.prepare_dummy_proof_operation(
                    storage,
                    ready_for_proof_blocks,
                    last_sealed_block,
                )
                .await
            }
            ProofSendingMode::OnlySampledProofs => {
                // if there is a sampled proof then send it, otherwise check for skipped ones.
                if let Some(op) = Self::load_real_proof_operation(storage, prover_storage).await {
                    Some(op)
                } else {
                    let ready_for_proof_blocks = storage
                        .blocks_dal()
                        .get_skipped_for_proof_blocks(limit)
                        .await;
                    self.prepare_dummy_proof_operation(
                        storage,
                        ready_for_proof_blocks,
                        last_sealed_block,
                    )
                    .await
                }
            }
        }
    }
}

async fn extract_ready_subrange(
    storage: &mut StorageProcessor<'_>,
    publish_criteria: &mut [Box<dyn BlockPublishCriterion>],
    unpublished_blocks: Vec<BlockWithMetadata>,
    last_sealed_block: L1BatchNumber,
) -> Option<Vec<BlockWithMetadata>> {
    let mut last_block: Option<L1BatchNumber> = None;
    for crit in publish_criteria.iter_mut() {
        if let Some(crit_block) = crit
            .last_block_to_publish(storage, &unpublished_blocks, last_sealed_block)
            .await
        {
            last_block = last_block.map_or(Some(crit_block), |block| Some(block.min(crit_block)));
        }
    }
    last_block.map(|last_block| {
        unpublished_blocks
            .into_iter()
            .take_while(|bl| bl.header.number <= last_block)
            .collect()
    })
}
