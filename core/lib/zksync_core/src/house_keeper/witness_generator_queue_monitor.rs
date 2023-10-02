use std::collections::HashMap;

use async_trait::async_trait;
use zksync_dal::ConnectionPool;
use zksync_types::proofs::{AggregationRound, JobCountStatistics};

use zksync_prover_utils::periodic_job::PeriodicJob;

const WITNESS_GENERATOR_SERVICE_NAME: &str = "witness_generator";

#[derive(Debug)]
pub struct WitnessGeneratorStatsReporter {
    reporting_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
}

impl WitnessGeneratorStatsReporter {
    pub fn new(reporting_interval_ms: u64, prover_connection_pool: ConnectionPool) -> Self {
        Self {
            reporting_interval_ms,
            prover_connection_pool,
        }
    }

    async fn get_job_statistics(
        prover_connection_pool: &ConnectionPool,
    ) -> HashMap<AggregationRound, JobCountStatistics> {
        let mut conn = prover_connection_pool.access_storage().await.unwrap();
        HashMap::from([
            (
                AggregationRound::BasicCircuits,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::BasicCircuits)
                    .await,
            ),
            (
                AggregationRound::LeafAggregation,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::LeafAggregation)
                    .await,
            ),
            (
                AggregationRound::NodeAggregation,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::NodeAggregation)
                    .await,
            ),
            (
                AggregationRound::Scheduler,
                conn.witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::Scheduler)
                    .await,
            ),
        ])
    }
}

fn emit_metrics_for_round(round: AggregationRound, stats: JobCountStatistics) {
    if stats.queued > 0 || stats.in_progress > 0 {
        tracing::trace!(
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
#[async_trait]
impl PeriodicJob for WitnessGeneratorStatsReporter {
    const SERVICE_NAME: &'static str = "WitnessGeneratorStatsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stats_for_all_rounds = Self::get_job_statistics(&self.prover_connection_pool).await;
        let mut aggregated = JobCountStatistics::default();
        for (round, stats) in stats_for_all_rounds {
            emit_metrics_for_round(round, stats);
            aggregated = aggregated + stats;
        }

        if aggregated.queued > 0 {
            tracing::trace!(
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
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
