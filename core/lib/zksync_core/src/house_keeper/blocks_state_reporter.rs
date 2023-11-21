use async_trait::async_trait;

use zksync_db_connection::ConnectionPool;
use zksync_prover_dal::ProverStorageProcessor;
use zksync_prover_utils::periodic_job::PeriodicJob;
use zksync_server_dal::ServerStorageProcessor;
use zksync_utils::time::seconds_since_epoch;

use crate::metrics::{BlockL1Stage, BlockStage, L1StageLatencyLabel, APP_METRICS};

#[derive(Debug)]
pub struct L1BatchMetricsReporter {
    reporting_interval_ms: u64,
    server_connection_pool: ConnectionPool,
    prover_connection_pool: ConnectionPool,
}

impl L1BatchMetricsReporter {
    pub fn new(
        reporting_interval_ms: u64,
        server_connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            reporting_interval_ms,
            server_connection_pool,
            prover_connection_pool,
        }
    }

    async fn report_metrics(&self) {
        let mut server_conn = self
            .server_connection_pool
            .access_storage::<ServerStorageProcessor>()
            .await
            .unwrap();
        let mut prover_conn = self
            .prover_connection_pool
            .access_storage::<ProverStorageProcessor>()
            .await
            .unwrap();
        let mut block_metrics = vec![
            (
                server_conn
                    .blocks_dal()
                    .get_sealed_l1_batch_number()
                    .await
                    .unwrap(),
                BlockStage::Sealed,
            ),
            (
                server_conn
                    .blocks_dal()
                    .get_last_l1_batch_number_with_metadata()
                    .await
                    .unwrap(),
                BlockStage::MetadataCalculated,
            ),
            (
                prover_conn
                    .witness_generator_dal()
                    .get_last_l1_batch_number_with_witness_inputs()
                    .await
                    .unwrap(),
                BlockStage::MerkleProofCalculated,
            ),
        ];

        let eth_stats = server_conn
            .eth_sender_dal()
            .get_eth_l1_batches()
            .await
            .unwrap();

        for (tx_type, l1_batch) in eth_stats.saved {
            let stage = BlockStage::L1 {
                l1_stage: BlockL1Stage::Saved,
                tx_type,
            };
            block_metrics.push((l1_batch, stage))
        }

        for (tx_type, l1_batch) in eth_stats.mined {
            let stage = BlockStage::L1 {
                l1_stage: BlockL1Stage::Mined,
                tx_type,
            };
            block_metrics.push((l1_batch, stage))
        }

        // TODO (PLA-335): Restore prover and witgen metrics

        for (l1_batch_number, stage) in block_metrics {
            APP_METRICS.block_number[&stage].set(l1_batch_number.0.into());
        }

        let oldest_uncommitted_batch_timestamp = server_conn
            .blocks_dal()
            .oldest_uncommitted_batch_timestamp()
            .await
            .unwrap();
        let oldest_unproved_batch_timestamp = server_conn
            .blocks_dal()
            .oldest_unproved_batch_timestamp()
            .await
            .unwrap();
        let oldest_unexecuted_batch_timestamp = server_conn
            .blocks_dal()
            .oldest_unexecuted_batch_timestamp()
            .await
            .unwrap();

        let now = seconds_since_epoch();

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
    }
}

#[async_trait]
impl PeriodicJob for L1BatchMetricsReporter {
    const SERVICE_NAME: &'static str = "L1BatchMetricsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        self.report_metrics().await;
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
