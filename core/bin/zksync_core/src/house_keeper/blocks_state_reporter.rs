use zksync_dal::ConnectionPool;

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct L1BatchMetricsReporter {
    reporting_interval_ms: u64,
}

impl L1BatchMetricsReporter {
    pub fn new(reporting_interval_ms: u64) -> Self {
        Self {
            reporting_interval_ms,
        }
    }

    fn report_metrics(&self, connection_pool: ConnectionPool) {
        let mut conn = connection_pool.access_storage_blocking();
        let mut block_metrics = vec![
            (
                conn.blocks_dal().get_sealed_block_number(),
                "sealed".to_string(),
            ),
            (
                conn.blocks_dal().get_last_block_number_with_metadata(),
                "metadata_calculated".to_string(),
            ),
            (
                conn.blocks_dal()
                    .get_last_l1_batch_number_with_witness_inputs(),
                "merkle_proof_calculated".to_string(),
            ),
        ];

        let eth_stats = conn.eth_sender_dal().get_eth_l1_batches();
        for (tx_type, l1_batch) in eth_stats.saved {
            block_metrics.push((l1_batch, format!("l1_saved_{:?}", tx_type)))
        }

        for (tx_type, l1_batch) in eth_stats.mined {
            block_metrics.push((l1_batch, format!("l1_mined_{:?}", tx_type)))
        }

        block_metrics.append(
            &mut conn
                .prover_dal()
                .get_proven_l1_batches()
                .into_iter()
                .map(|(l1_batch_number, stage)| (l1_batch_number, format!("prove_{:?}", stage)))
                .collect(),
        );

        block_metrics.append(
            &mut conn
                .witness_generator_dal()
                .get_witness_generated_l1_batches()
                .into_iter()
                .map(|(l1_batch_number, stage)| (l1_batch_number, format!("wit_gen_{:?}", stage)))
                .collect(),
        );

        for (l1_batch_number, stage) in block_metrics {
            metrics::gauge!(
                "server.block_number",
                l1_batch_number.0 as f64,
                "stage" =>  stage
            );
        }
    }
}

impl PeriodicJob for L1BatchMetricsReporter {
    const SERVICE_NAME: &'static str = "L1BatchMetricsReporter";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        self.report_metrics(connection_pool);
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
