use std::collections::HashMap;

use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::JobCountStatistics, ProtocolVersionId,
};

use crate::periodic_job::PeriodicJob;

const FRI_WITNESS_GENERATOR_SERVICE_NAME: &str = "fri_witness_generator";

/// `FriWitnessGeneratorQueueReporter` is a task that periodically reports witness generator jobs status.
/// Note: these values will be used for auto-scaling witness generators (Basic, Leaf, Node, Recursion Tip and Scheduler).
#[derive(Debug)]
pub struct FriWitnessGeneratorQueueReporter {
    reporting_interval_ms: u64,
    pool: ConnectionPool<Prover>,
}

impl FriWitnessGeneratorQueueReporter {
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

    metrics::gauge!(
        format!("server.{}.jobs", FRI_WITNESS_GENERATOR_SERVICE_NAME),
        stats.queued as f64,
        "type" => "queued",
        "round" => format!("{:?}", round)
    );

    metrics::gauge!(
        format!("server.{}.jobs", FRI_WITNESS_GENERATOR_SERVICE_NAME),
        stats.in_progress as f64,
        "type" => "in_progress",
        "round" => format!("{:?}", round)
    );
}

#[async_trait]
impl PeriodicJob for FriWitnessGeneratorQueueReporter {
    const SERVICE_NAME: &'static str = "FriWitnessGeneratorQueueReporter";

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

        metrics::gauge!(
            format!("server.{}.jobs", FRI_WITNESS_GENERATOR_SERVICE_NAME),
            aggregated.queued as f64,
            "type" => "queued",
            "protocol_version" => ProtocolVersionId::current_prover_version().to_string(),
        );

        metrics::gauge!(
            format!("server.{}.jobs", FRI_WITNESS_GENERATOR_SERVICE_NAME),
            aggregated.in_progress as f64,
            "type" => "in_progress",
            "protocol_version" => ProtocolVersionId::current_prover_version().to_string(),
        );
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
