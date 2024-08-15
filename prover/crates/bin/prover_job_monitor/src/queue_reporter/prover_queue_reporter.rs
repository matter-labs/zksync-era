use async_trait::async_trait;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::{basic_fri_types::CircuitIdRoundTuple, prover_dal::JobCountStatistics};

use crate::metrics::{JobStatus, PROVER_JOB_MONITOR_METRICS};

/// `ProverQueueReporter` is a task that periodically reports prover jobs status.
/// Note: these values will be used for auto-scaling provers and Witness Vector Generators.
#[derive(Debug)]
pub struct ProverQueueReporter {
    connection_pool: ConnectionPool<Prover>,
    config: FriProverGroupConfig,
    /// time between each run
    run_interval_ms: u64,
}

impl ProverQueueReporter {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        config: FriProverGroupConfig,
        run_interval_ms: u64,
    ) -> Self {
        Self {
            connection_pool,
            config,
            run_interval_ms,
        }
    }
}

#[async_trait]
impl PeriodicJob for ProverQueueReporter {
    const SERVICE_NAME: &'static str = "ProverQueueReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await.unwrap();
        let stats = conn.fri_prover_jobs_dal().get_prover_jobs_stats().await;

        for (protocol_semantic_version, circuit_prover_stats) in stats {
            for (
                CircuitIdRoundTuple {
                    circuit_id,
                    aggregation_round,
                },
                JobCountStatistics {
                    queued,
                    in_progress,
                },
            ) in circuit_prover_stats
            {
                let group_id = self
                    .config
                    .get_group_id_for_circuit_id_and_aggregation_round(
                        circuit_id,
                        aggregation_round,
                    )
                    .unwrap_or(u8::MAX);

                PROVER_JOB_MONITOR_METRICS.report_prover_jobs(
                    JobStatus::Queued,
                    circuit_id,
                    aggregation_round,
                    group_id,
                    protocol_semantic_version,
                    queued as u64,
                );

                PROVER_JOB_MONITOR_METRICS.report_prover_jobs(
                    JobStatus::InProgress,
                    circuit_id,
                    aggregation_round,
                    group_id,
                    protocol_semantic_version,
                    in_progress as u64,
                )
            }
        }

        let lag_by_circuit_type = conn
            .fri_prover_jobs_dal()
            .min_unproved_l1_batch_number()
            .await;

        for ((circuit_id, aggregation_round), l1_batch_number) in lag_by_circuit_type {
            PROVER_JOB_MONITOR_METRICS.oldest_unprocessed_batch
                [&(circuit_id.to_string(), aggregation_round.to_string())]
                .set(l1_batch_number.0 as u64);
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.run_interval_ms
    }
}
