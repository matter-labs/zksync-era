use async_trait::async_trait;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::JobCountStatistics,
};

use crate::metrics::{JobStatus, PROVER_JOB_MONITOR_METRICS};

/// `WitnessGeneratorQueueReporter` is a task that periodically reports witness generator jobs status.
/// Note: these values will be used for auto-scaling witness generators (Basic, Leaf, Node, Recursion Tip and Scheduler).
#[derive(Debug)]
pub struct WitnessGeneratorQueueReporter {
    pool: ConnectionPool<Prover>,
    /// time between each run
    run_interval_ms: u64,
}

impl WitnessGeneratorQueueReporter {
    pub fn new(pool: ConnectionPool<Prover>, run_interval_ms: u64) -> Self {
        Self {
            pool,
            run_interval_ms,
        }
    }
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

    PROVER_JOB_MONITOR_METRICS.witness_generator_jobs_by_round[&(
        JobStatus::Queued,
        round.to_string(),
        protocol_version.to_string(),
    )]
        .set(stats.queued as u64);
    PROVER_JOB_MONITOR_METRICS.witness_generator_jobs_by_round[&(
        JobStatus::InProgress,
        round.to_string(),
        protocol_version.to_string(),
    )]
        .set(stats.in_progress as u64);
}

#[async_trait]
impl PeriodicJob for WitnessGeneratorQueueReporter {
    const SERVICE_NAME: &'static str = "WitnessGeneratorQueueReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.pool.connection().await.unwrap();

        for round in AggregationRound::all_rounds() {
            let stats = conn
                .fri_witness_generator_dal()
                .get_witness_jobs_stats(round)
                .await;
            for ((round, semantic_protocol_version), job_stats) in stats {
                emit_metrics_for_round(round, semantic_protocol_version, &job_stats);
            }
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}
