use zksync_dal::ConnectionPool;
use zksync_types::proofs::{AggregationRound, JobCountStatistics};

use crate::house_keeper::periodic_job::PeriodicJob;

const WITNESS_GENERATOR_SERVICE_NAME: &str = "witness_generator";

#[derive(Debug, Default)]
pub struct WitnessGeneratorStatsReporter {}

impl WitnessGeneratorStatsReporter {
    fn get_job_statistics(connection_pool: ConnectionPool) -> JobCountStatistics {
        let mut conn = connection_pool.access_storage_blocking();
        conn.witness_generator_dal()
            .get_witness_jobs_stats(AggregationRound::BasicCircuits)
            + conn
                .witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::LeafAggregation)
            + conn
                .witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::NodeAggregation)
            + conn
                .witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::Scheduler)
    }
}

/// Invoked periodically to push job statistics to Prometheus
/// Note: these values will be used for auto-scaling job processors
impl PeriodicJob for WitnessGeneratorStatsReporter {
    const SERVICE_NAME: &'static str = "WitnessGeneratorStatsReporter";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        let stats = Self::get_job_statistics(connection_pool);

        vlog::info!("Found {} free witness generators jobs", stats.queued);

        metrics::gauge!(
            format!("server.{}.jobs", WITNESS_GENERATOR_SERVICE_NAME),
            stats.queued as f64,
            "type" => "queued"
        );

        metrics::gauge!(
            format!("server.{}.jobs", WITNESS_GENERATOR_SERVICE_NAME),
            stats.in_progress as f64,
            "type" => "in_progress"
        );
    }
}
