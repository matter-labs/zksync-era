use std::collections::HashMap;

use zksync_dal::ConnectionPool;
use zksync_types::proofs::{AggregationRound, JobCountStatistics};

use crate::house_keeper::periodic_job::PeriodicJob;

const WITNESS_GENERATOR_SERVICE_NAME: &str = "witness_generator";

#[derive(Debug)]
pub struct WitnessGeneratorStatsReporter {
    reporting_interval_ms: u64,
}

impl WitnessGeneratorStatsReporter {
    pub fn new(reporting_interval_ms: u64) -> Self {
        Self {
            reporting_interval_ms,
        }
    }

    fn get_job_statistics(
        connection_pool: ConnectionPool,
    ) -> HashMap<AggregationRound, JobCountStatistics> {
        let mut conn = connection_pool.access_storage_blocking();
        HashMap::from([
            (
                AggregationRound::BasicCircuits,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::BasicCircuits),
            ),
            (
                AggregationRound::LeafAggregation,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::LeafAggregation),
            ),
            (
                AggregationRound::NodeAggregation,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::NodeAggregation),
            ),
            (
                AggregationRound::Scheduler,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::Scheduler),
            ),
        ])
    }
}

fn emit_metrics_for_round(round: AggregationRound, stats: JobCountStatistics) {
    if stats.queued > 0 || stats.in_progress > 0 {
        vlog::trace!(
            "Found {} free and {} in progress {:?} witness generators jobs",
            stats.queued,
            stats.in_progress,
            round
        );
    }

    metrics::gauge!(
        format!("server.{}.jobs", WITNESS_GENERATOR_SERVICE_NAME),
        stats.queued as f64,
        "type" => "queued",
        "round" => format!("{:?}", round)
    );

    metrics::gauge!(
        format!("server.{}.jobs", WITNESS_GENERATOR_SERVICE_NAME),
        stats.in_progress as f64,
        "type" => "in_progress",
        "round" => format!("{:?}", round)
    );
}

/// Invoked periodically to push job statistics to Prometheus
/// Note: these values will be used for auto-scaling job processors
impl PeriodicJob for WitnessGeneratorStatsReporter {
    const SERVICE_NAME: &'static str = "WitnessGeneratorStatsReporter";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        let stats_for_all_rounds = Self::get_job_statistics(connection_pool);
        let mut aggregated = JobCountStatistics::default();
        for (round, stats) in stats_for_all_rounds {
            emit_metrics_for_round(round, stats);
            aggregated = aggregated + stats;
        }

        if aggregated.queued > 0 {
            vlog::trace!(
                "Found {} free {} in progress witness generators jobs",
                aggregated.queued,
                aggregated.in_progress
            );
        }

        metrics::gauge!(
            format!("server.{}.jobs", WITNESS_GENERATOR_SERVICE_NAME),
            aggregated.queued as f64,
            "type" => "queued"
        );

        metrics::gauge!(
            format!("server.{}.jobs", WITNESS_GENERATOR_SERVICE_NAME),
            aggregated.in_progress as f64,
            "type" => "in_progress"
        );
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
