use async_trait::async_trait;
use zksync_prover_dal::{Connection, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::JobCountStatistics,
};

use crate::{metrics::SERVER_METRICS, task_wiring::Task};

/// `WitnessGeneratorQueueReporter` is a task that reports witness generator jobs status.
/// Note: these values will be used for auto-scaling witness generators (Basic, Leaf, Node, Recursion Tip and Scheduler).
#[derive(Debug)]
pub struct WitnessGeneratorQueueReporter;

impl WitnessGeneratorQueueReporter {
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

        SERVER_METRICS.witness_generator_jobs_by_round
            [&("queued", round.to_string(), protocol_version.to_string())]
            .set(stats.queued as u64);
        SERVER_METRICS.witness_generator_jobs_by_round[&(
            "in_progress",
            round.to_string(),
            protocol_version.to_string(),
        )]
            .set(stats.in_progress as u64);
    }
}

#[async_trait]
impl Task for WitnessGeneratorQueueReporter {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()> {
        for round in AggregationRound::ALL_ROUNDS {
            let stats = connection
                .fri_witness_generator_dal()
                .get_witness_jobs_stats(round)
                .await;
            for ((round, semantic_protocol_version), job_stats) in stats {
                Self::emit_metrics_for_round(round, semantic_protocol_version, &job_stats);
            }
        }

        Ok(())
    }
}
