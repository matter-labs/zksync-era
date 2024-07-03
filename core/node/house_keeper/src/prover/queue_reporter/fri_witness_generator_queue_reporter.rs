use std::collections::HashMap;

use async_trait::async_trait;
use zksync_dal::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::JobCountStatistics,
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
    ) -> HashMap<(AggregationRound, ProtocolSemanticVersion), JobCountStatistics> {
        let mut conn = self.pool.connection().await.unwrap();
        let mut result = HashMap::new();
        result.extend(
            conn.fri_witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::BasicCircuits)
                .await,
        );
        result.extend(
            conn.fri_witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::LeafAggregation)
                .await,
        );
        result.extend(
            conn.fri_witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::NodeAggregation)
                .await,
        );
        result.extend(
            conn.fri_witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::RecursionTip)
                .await,
        );
        result.extend(
            conn.fri_witness_generator_dal()
                .get_witness_jobs_stats(AggregationRound::Scheduler)
                .await,
        );

        result
    }
}

fn emit_metrics_for_round(
    round: AggregationRound,
    protocol_version: ProtocolSemanticVersion,
    stats: &JobCountStatistics,
) {
    if stats.queued > 0 || stats.in_progress > 0 {
        tracing::trace!(
            "Found {} free and {} in progress {:?} FRI witness generators jobs for protocol version {}",
            stats.queued,
            stats.in_progress,
            round,
            protocol_version
        );
    }

    SERVER_METRICS.witness_generator_jobs_by_round[&(
        "queued",
        format!("{:?}", round),
        protocol_version.to_string(),
    )]
        .set(stats.queued as u64);
    SERVER_METRICS.witness_generator_jobs_by_round[&(
        "in_progress",
        format!("{:?}", round),
        protocol_version.to_string(),
    )]
        .set(stats.in_progress as u64);
}

#[async_trait]
impl PeriodicJob for FriWitnessGeneratorQueueReporter {
    const SERVICE_NAME: &'static str = "FriWitnessGeneratorQueueReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let stats_for_all_rounds = self.get_job_statistics().await;
        let mut aggregated = HashMap::<ProtocolSemanticVersion, JobCountStatistics>::new();
        for ((round, protocol_version), stats) in stats_for_all_rounds {
            emit_metrics_for_round(round, protocol_version, &stats);

            let entry = aggregated.entry(protocol_version).or_default();
            entry.queued += stats.queued;
            entry.in_progress += stats.in_progress;
        }

        for (protocol_version, stats) in &aggregated {
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
