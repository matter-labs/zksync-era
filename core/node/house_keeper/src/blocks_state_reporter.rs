use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_shared_metrics::{BlockStage, L1Stage, L1StageLatencyLabel, L2BlockStage, APP_METRICS};
use zksync_types::{aggregated_operations::AggregatedActionType, L2BlockNumber};

use crate::{metrics::FRI_PROVER_METRICS, periodic_job::PeriodicJob};

/// Reports l2 blocks and l1 batches metrics to the prometheus.
#[derive(Debug)]
pub struct BlockMetricsReporter {
    reporting_interval_ms: u64,
    connection_pool: ConnectionPool<Core>,
}

impl BlockMetricsReporter {
    pub fn new(reporting_interval_ms: u64, connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            reporting_interval_ms,
            connection_pool,
        }
    }

    async fn report_metrics(&self) -> anyhow::Result<()> {
        let mut batch_metrics = vec![];
        let mut conn = self
            .connection_pool
            .connection_tagged("house_keeper")
            .await?;
        let last_l1_batch = conn.blocks_dal().get_sealed_l1_batch_number().await?;
        if let Some(number) = last_l1_batch {
            batch_metrics.push((number, BlockStage::Sealed));
        }

        let last_l1_batch_with_tree_data = conn
            .blocks_dal()
            .get_last_l1_batch_number_with_tree_data()
            .await?;
        if let Some(number) = last_l1_batch_with_tree_data {
            batch_metrics.push((number, BlockStage::MetadataCalculated));
        }

        let mut l2_block_metircs: Vec<(L2BlockNumber, L2BlockStage)> = vec![];
        let eth_stats = conn.eth_sender_dal().get_eth_all_blocks_stat().await?;

        let mut add_metric = |l1_stage: L1Stage, list: Vec<(AggregatedActionType, u32)>| {
            for (tx_type, block) in list {
                match tx_type {
                    AggregatedActionType::L2Block(tx_type) => {
                        let stage = L2BlockStage::L1 { l1_stage, tx_type };
                        l2_block_metircs.push((block.into(), stage))
                    }
                    AggregatedActionType::L1Batch(tx_type) => {
                        let stage = BlockStage::L1 { l1_stage, tx_type };
                        batch_metrics.push((block.into(), stage))
                    }
                }
            }
        };
        add_metric(L1Stage::Saved, eth_stats.saved);
        add_metric(L1Stage::Mined, eth_stats.mined);

        // TODO (PLA-335): Restore prover and witgen metrics

        for (l1_batch_number, stage) in batch_metrics {
            APP_METRICS.block_number[&stage].set(l1_batch_number.0.into());
        }

        for (l2_block, stage) in l2_block_metircs {
            APP_METRICS.miniblock_number[&stage].set(l2_block.0.into());
        }

        let oldest_uncommitted_batch_timestamp = conn
            .blocks_dal()
            .oldest_uncommitted_batch_timestamp()
            .await?;
        let oldest_unproved_batch_timestamp =
            conn.blocks_dal().oldest_unproved_batch_timestamp().await?;
        let oldest_unexecuted_batch_timestamp = conn
            .blocks_dal()
            .oldest_unexecuted_batch_timestamp()
            .await?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Incorrect system time")
            .as_secs();

        if let Some(timestamp) = oldest_uncommitted_batch_timestamp {
            APP_METRICS.blocks_state_block_eth_stage_latency
                [&L1StageLatencyLabel::UncommittedBlock]
                .set(now.saturating_sub(timestamp));
        }
        if let Some(timestamp) = oldest_unproved_batch_timestamp {
            APP_METRICS.blocks_state_block_eth_stage_latency[&L1StageLatencyLabel::UnprovedBlock]
                .set(now.saturating_sub(timestamp));
        }
        if let Some(timestamp) = oldest_unexecuted_batch_timestamp {
            APP_METRICS.blocks_state_block_eth_stage_latency[&L1StageLatencyLabel::UnexecutedBlock]
                .set(now.saturating_sub(timestamp));
        }

        // proof generation details metrics
        let oldest_unpicked_batch = match conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await?
        {
            Some(l1_batch_number) => l1_batch_number.0 as u64,
            // if there is no unpicked batch in database, we use sealed batch number as a result
            None => {
                conn.blocks_dal()
                    .get_sealed_l1_batch_number()
                    .await
                    .unwrap()
                    .unwrap()
                    .0 as u64
            }
        };
        FRI_PROVER_METRICS
            .oldest_unpicked_batch
            .set(oldest_unpicked_batch);

        if let Some(l1_batch_number) = conn
            .proof_generation_dal()
            .get_oldest_not_generated_batch()
            .await?
        {
            FRI_PROVER_METRICS
                .oldest_not_generated_batch
                .set(l1_batch_number.0 as u64);
        }
        Ok(())
    }
}

#[async_trait]
impl PeriodicJob for BlockMetricsReporter {
    const SERVICE_NAME: &'static str = "L1BatchMetricsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.report_metrics().await
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
