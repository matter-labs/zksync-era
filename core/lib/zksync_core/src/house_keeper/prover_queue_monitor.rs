use async_trait::async_trait;
use zksync_config::configs::ProverGroupConfig;
use zksync_dal::ConnectionPool;
use zksync_prover_utils::circuit_name_to_numeric_index;

use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct ProverStatsReporter {
    reporting_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
    config: ProverGroupConfig,
}

impl ProverStatsReporter {
    pub fn new(
        reporting_interval_ms: u64,
        prover_connection_pool: ConnectionPool,
        config: ProverGroupConfig,
    ) -> Self {
        Self {
            reporting_interval_ms,
            prover_connection_pool,
            config,
        }
    }
}

/// Invoked periodically to push job statistics to Prometheus
/// Note: these values will be used for manually scaling provers.
#[async_trait]
impl PeriodicJob for ProverStatsReporter {
    const SERVICE_NAME: &'static str = "ProverStatsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.prover_connection_pool.access_storage().await.unwrap();
        let stats = conn.prover_dal().get_prover_jobs_stats_per_circuit().await;

        for (circuit_name, stats) in stats.into_iter() {
            let group_id = self
                .config
                .get_group_id_for_circuit_id(circuit_name_to_numeric_index(&circuit_name).unwrap())
                .unwrap();

            metrics::gauge!(
              "server.prover.jobs",
              stats.queued as f64,
              "type" => "queued",
              "prover_group_id" => group_id.to_string(),
              "circuit_name" => circuit_name.clone(),
              "circuit_type" => circuit_name_to_numeric_index(&circuit_name).unwrap().to_string()
            );

            metrics::gauge!(
              "server.prover.jobs",
              stats.in_progress as f64,
              "type" => "in_progress",
              "prover_group_id" => group_id.to_string(),
              "circuit_name" => circuit_name.clone(),
              "circuit_type" => circuit_name_to_numeric_index(&circuit_name).unwrap().to_string()
            );
        }

        if let Some(min_unproved_l1_batch_number) =
            conn.prover_dal().min_unproved_l1_batch_number().await
        {
            metrics::gauge!("server.block_number", min_unproved_l1_batch_number.0 as f64, "stage" => "circuit_aggregation")
        }

        let lag_by_circuit_type = conn
            .prover_dal()
            .min_unproved_l1_batch_number_by_basic_circuit_type()
            .await;

        for (circuit_type, l1_batch_number) in lag_by_circuit_type {
            metrics::gauge!("server.block_number", l1_batch_number.0 as f64, "stage" => format!("circuit_{}", circuit_type));
        }
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
