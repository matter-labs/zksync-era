use std::collections::HashMap;

use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;
use zksync_types::{basic_fri_types::AggregationRound, prover_dal::JobCountStatistics};

use crate::{metrics::SERVER_METRICS, periodic_job::PeriodicJob};

#[derive(Debug)]
pub struct FriWitnessGeneratorStatsReporter {
    reporting_interval_ms: u64,
    pool: ConnectionPool<Prover>,
}

impl FriWitnessGeneratorStatsReporter {
    pub fn new(pool: ConnectionPool<Prover>, reporting_interval_ms: u64) -> Self {
        Self {
            reporting_interval_ms,
            pool,
        }
    }

    async fn get_job_statistics(&self) -> HashMap<AggregationRound, JobCountStatistics> {
        let mut conn = self.pool.connection().await.unwrap();
        HashMap::from([
            (
                AggregationRound::BasicCircuits,
                conn.fri_witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::BasicCircuits)
                    .await,
            ),
            (
                AggregationRound::LeafAggregation,
                conn.fri_witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::LeafAggregation)
                    .await,
            ),
            (
                AggregationRound::NodeAggregation,
                conn.fri_witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::NodeAggregation)
                    .await,
            ),
            (
                AggregationRound::RecursionTip,
                conn.fri_witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::RecursionTip)
                    .await,
            ),
            (
                AggregationRound::Scheduler,
                conn.fri_witness_generator_dal()
                    .get_witness_jobs_stats(AggregationRound::Scheduler)
                    .await,
            ),
        ])
    }
}

fn emit_metrics_for_round(round: AggregationRound, stats: JobCountStatistics) {
    if stats.queued > 0 || stats.in_progress > 0 {
        tracing::trace!(
            "Found {} free and {} in progress {:?} FRI witness generators jobs",
            stats.queued,
            stats.in_progress,
            round
        );
    }

    SERVER_METRICS.witness_generator_jobs_by_round[&("queued", format!("{:?}", round))]
        .set(stats.queued as u64);
    SERVER_METRICS.witness_generator_jobs_by_round[&("in_progress", format!("{:?}", round))]
        .set(stats.queued as u64);
}

/// Invoked periodically to push job statistics to Prometheus
/// Note: these values will be used for auto-scaling job processors
#[async_trait]
impl PeriodicJob for FriWitnessGeneratorStatsReporter {
    const SERVICE_NAME: &'static str = "WitnessGeneratorStatsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stats_for_all_rounds = self.get_job_statistics().await;
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

        SERVER_METRICS.witness_generator_jobs[&("queued")].set(aggregated.queued as u64);
        SERVER_METRICS.witness_generator_jobs[&("in_progress")].set(aggregated.in_progress as u64);

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
