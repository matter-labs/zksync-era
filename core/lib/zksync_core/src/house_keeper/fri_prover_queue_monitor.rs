use async_trait::async_trait;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_dal::ConnectionPool;
use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct FriProverStatsReporter {
    reporting_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
    config: FriProverGroupConfig,
}

impl FriProverStatsReporter {
    pub fn new(
        reporting_interval_ms: u64,
        prover_connection_pool: ConnectionPool,
        config: FriProverGroupConfig,
    ) -> Self {
        Self {
            reporting_interval_ms,
            prover_connection_pool,
            config,
        }
    }
}

///  Invoked periodically to push prover queued/inprogress job statistics
#[async_trait]
impl PeriodicJob for FriProverStatsReporter {
    const SERVICE_NAME: &'static str = "FriProverStatsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.prover_connection_pool.access_storage().await.unwrap();
        let stats = conn.fri_prover_jobs_dal().get_prover_jobs_stats().await;

        for ((circuit_id, aggregation_round), stats) in stats.into_iter() {
            let group_id = self
                .config
                .get_group_id_for_circuit_id_and_aggregation_round(circuit_id, aggregation_round)
                .unwrap_or(u8::MAX);

            metrics::gauge!(
              "fri_prover.prover.jobs",
              stats.queued as f64,
              "type" => "queued",
              "circuit_id" => circuit_id.to_string(),
              "aggregation_round" => aggregation_round.to_string(),
              "prover_group_id" => group_id.to_string(),
            );

            metrics::gauge!(
              "fri_prover.prover.jobs",
              stats.in_progress as f64,
              "type" => "in_progress",
              "circuit_id" => circuit_id.to_string(),
              "aggregation_round" => aggregation_round.to_string(),
              "prover_group_id" => group_id.to_string(),
            );
        }

        let lag_by_circuit_type = conn
            .fri_prover_jobs_dal()
            .min_unproved_l1_batch_number()
            .await;

        for ((circuit_id, aggregation_round), l1_batch_number) in lag_by_circuit_type {
            metrics::gauge!(
              "fri_prover.block_number", l1_batch_number.0 as f64,
              "circuit_id" => circuit_id.to_string(),
              "aggregation_round" => aggregation_round.to_string());
        }
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
