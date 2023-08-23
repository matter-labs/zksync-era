use async_trait::async_trait;

use zksync_dal::ConnectionPool;
use zksync_utils::time::seconds_since_epoch;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct L1BatchMetricsReporter {
    reporting_interval_ms: u64,
    connection_pool: ConnectionPool,
}

impl L1BatchMetricsReporter {
    pub fn new(reporting_interval_ms: u64, connection_pool: ConnectionPool) -> Self {
        Self {
            reporting_interval_ms,
            connection_pool,
        }
    }

    async fn report_metrics(&self) {
        let mut conn = self.connection_pool.access_storage().await;
        let mut block_metrics = vec![
            (
                conn.blocks_dal().get_sealed_l1_batch_number().await,
                "sealed".to_string(),
            ),
            (
                conn.blocks_dal()
                    .get_last_l1_batch_number_with_metadata()
                    .await,
                "metadata_calculated".to_string(),
            ),
            (
                conn.blocks_dal()
                    .get_last_l1_batch_number_with_witness_inputs()
                    .await,
                "merkle_proof_calculated".to_string(),
            ),
        ];

        let eth_stats = conn.eth_sender_dal().get_eth_l1_batches().await;
        for (tx_type, l1_batch) in eth_stats.saved {
            block_metrics.push((l1_batch, format!("l1_saved_{}", tx_type.as_str())))
        }

        for (tx_type, l1_batch) in eth_stats.mined {
            block_metrics.push((l1_batch, format!("l1_mined_{}", tx_type.as_str())))
        }

        for (l1_batch_number, stage) in block_metrics {
            metrics::gauge!(
                "server.block_number",
                l1_batch_number.0 as f64,
                "stage" => stage
            );
        }

        let oldest_uncommitted_batch_timestamp =
            conn.blocks_dal().oldest_uncommitted_batch_timestamp().await;
        let oldest_unproved_batch_timestamp =
            conn.blocks_dal().oldest_unproved_batch_timestamp().await;
        let oldest_unexecuted_batch_timestamp =
            conn.blocks_dal().oldest_unexecuted_batch_timestamp().await;

        let now = seconds_since_epoch();

        if let Some(timestamp) = oldest_uncommitted_batch_timestamp {
            metrics::gauge!(
                "server.blocks_state.block_eth_stage_latency",
                now.saturating_sub(timestamp) as f64,
                "stage" => "uncommitted_block"
            );
        }

        if let Some(timestamp) = oldest_unproved_batch_timestamp {
            metrics::gauge!(
                "server.blocks_state.block_eth_stage_latency",
                now.saturating_sub(timestamp) as f64,
                "stage" => "unproved_block"
            );
        }

        if let Some(timestamp) = oldest_unexecuted_batch_timestamp {
            metrics::gauge!(
                "server.blocks_state.block_eth_stage_latency",
                now.saturating_sub(timestamp) as f64,
                "stage" => "unexecuted_block"
            );
        }
    }
}

#[async_trait]
impl PeriodicJob for L1BatchMetricsReporter {
    const SERVICE_NAME: &'static str = "L1BatchMetricsReporter";

    async fn run_routine_task(&mut self) {
        self.report_metrics().await;
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
