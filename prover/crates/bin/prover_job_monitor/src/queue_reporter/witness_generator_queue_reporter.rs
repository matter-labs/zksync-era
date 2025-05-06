use anyhow::Context;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::JobCountStatistics,
};

use crate::metrics::SERVER_METRICS;

/// `WitnessGeneratorQueueReporter` is a task that reports witness generator jobs status.
///
/// Note: these values will be used for auto-scaling witness generators
/// (Basic, Leaf, Node, Recursion Tip and Scheduler).
#[derive(Debug)]
pub struct WitnessGeneratorQueueReporter {
    pool: ConnectionPool<Prover>,
}

impl WitnessGeneratorQueueReporter {
    pub fn new(pool: ConnectionPool<Prover>) -> Self {
        Self { pool }
    }

    fn emit_metrics_for_round(
        round: AggregationRound,
        protocol_version: ProtocolSemanticVersion,
        stats: &JobCountStatistics,
    ) {
        if stats.queued > 0 {
            tracing::info!(
                "Found {} queued {} witness generator jobs for protocol version {}.",
                stats.queued,
                round,
                protocol_version
            );
        }
        if stats.in_progress > 0 {
            tracing::info!(
                "Found {} in progress {} witness generator jobs for protocol version {}.",
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
}

#[async_trait::async_trait]
impl Task for WitnessGeneratorQueueReporter {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        for round in AggregationRound::ALL_ROUNDS {
            let stats = connection
                .fri_witness_generator_dal()
                .get_witness_jobs_stats(round)
                .await;
            for (semantic_protocol_version, job_stats) in stats {
                Self::emit_metrics_for_round(round, semantic_protocol_version, &job_stats);
            }
        }

        Ok(())
    }
}
