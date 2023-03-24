use std::collections::HashMap;

use zksync_config::configs::ProverGroupConfig;
use zksync_dal::ConnectionPool;
use zksync_prover_utils::circuit_name_to_numeric_index;
use zksync_types::proofs::JobCountStatistics;

use crate::house_keeper::periodic_job::PeriodicJob;

const PROVER_SERVICE_NAME: &str = "prover";

#[derive(Debug, Default)]
pub struct ProverStatsReporter {}

impl ProverStatsReporter {
    fn get_job_statistics(connection_pool: ConnectionPool) -> HashMap<String, JobCountStatistics> {
        let mut conn = connection_pool.access_storage_blocking();
        conn.prover_dal().get_prover_jobs_stats_per_circuit()
    }
}

/// Invoked periodically to push job statistics to Prometheus
/// Note: these values will be used for manually scaling provers.
impl PeriodicJob for ProverStatsReporter {
    const SERVICE_NAME: &'static str = "ProverStatsReporter";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        let prover_group_config = ProverGroupConfig::from_env();
        let stats = Self::get_job_statistics(connection_pool);
        let prover_group_to_stats: HashMap<u8, JobCountStatistics> = stats
            .into_iter()
            .map(|(key, value)| {
                (
                    prover_group_config
                        .get_group_id_for_circuit_id(circuit_name_to_numeric_index(&key).unwrap())
                        .unwrap(),
                    value,
                )
            })
            .collect();
        for (group_id, stats) in prover_group_to_stats.into_iter() {
            metrics::gauge!(
              format!("server.{}.jobs", PROVER_SERVICE_NAME),
              stats.queued as f64,
              "type" => "queued",
              "prover_group_id" => group_id.to_string(),
            );

            metrics::gauge!(
              format!("server.{}.jobs", PROVER_SERVICE_NAME),
              stats.in_progress as f64,
              "type" => "in_progress", "prover_group_id" => group_id.to_string(),
            );
        }
    }
}
