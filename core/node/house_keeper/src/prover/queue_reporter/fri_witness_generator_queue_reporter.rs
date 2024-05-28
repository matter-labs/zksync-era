use std::collections::HashMap;

use async_trait::async_trait;
use prover_dal::{Prover, ProverDal};
use zksync_dal::ConnectionPool;
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{JobCountStatistics, JobCountStatisticsByProtocolVersion},
    ProtocolVersionId,
};

use crate::{periodic_job::PeriodicJob, prover::metrics::SERVER_METRICS};

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

    async fn get_job_statistics(
        &self,
    ) -> HashMap<AggregationRound, Vec<JobCountStatisticsByProtocolVersion>> {
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

fn emit_metrics_for_round(
    round: AggregationRound,
    stats: Vec<JobCountStatisticsByProtocolVersion>,
) {
    for stats in stats.iter() {
        if stats.queued > 0 || stats.in_progress > 0 {
            tracing::trace!(
                "Found {} free and {} in progress {:?} FRI witness generators jobs for protocol version {}",
                stats.queued,
                stats.in_progress,
                round,
                stats.protocol_version
            );
        }

        SERVER_METRICS.witness_generator_jobs_by_round[&(
            "queued",
            format!("{:?}", round),
            stats.protocol_version.to_string(),
        )]
            .set(stats.queued as u64);
        SERVER_METRICS.witness_generator_jobs_by_round[&(
            "in_progress",
            format!("{:?}", round),
            stats.protocol_version.to_string(),
        )]
            .set(stats.in_progress as u64);
    }
}

#[async_trait]
impl PeriodicJob for FriWitnessGeneratorQueueReporter {
    const SERVICE_NAME: &'static str = "FriWitnessGeneratorQueueReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stats_for_all_rounds = self.get_job_statistics().await;
        let mut aggregated = HashMap::<ProtocolVersionId, JobCountStatistics>::new();
        for (round, stats) in stats_for_all_rounds {
            emit_metrics_for_round(round, stats.clone());

            for stats in stats.iter() {
                let entry = aggregated.entry(stats.protocol_version).or_insert_with(|| {
                    JobCountStatistics {
                        queued: 0,
                        in_progress: 0,
                    }
                });
                entry.queued += stats.queued;
                entry.in_progress += stats.in_progress;
            }
        }

        for aggregated in aggregated.iter() {
            let protocol_version = aggregated.0;
            let stats = aggregated.1;
            if stats.queued > 0 || stats.in_progress > 0 {
                tracing::trace!(
                    "Found {} free {} in progress witness generators jobs for protocol version {}",
                    stats.queued,
                    stats.in_progress,
                    protocol_version
                );
            }

            SERVER_METRICS.witness_generator_jobs[&("queued", protocol_version.to_string())]
                .set(stats.queued as u64);

            SERVER_METRICS.witness_generator_jobs[&("in_progress", protocol_version.to_string())]
                .set(stats.in_progress as u64);
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
