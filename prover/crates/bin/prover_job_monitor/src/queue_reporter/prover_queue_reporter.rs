use async_trait::async_trait;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_prover_dal::{Connection, Prover, ProverDal};
use zksync_types::{basic_fri_types::CircuitIdRoundTuple, prover_dal::JobCountStatistics};

use crate::{metrics::FRI_PROVER_METRICS, task_wiring::Task};

/// `ProverQueueReporter` is a task that reports prover jobs status.
/// Note: these values will be used for auto-scaling provers and Witness Vector Generators.
#[derive(Debug)]
pub struct ProverQueueReporter {
    config: FriProverGroupConfig,
}

impl ProverQueueReporter {
    pub fn new(config: FriProverGroupConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Task for ProverQueueReporter {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()> {
        let stats = connection
            .fri_prover_jobs_dal()
            .get_prover_jobs_stats()
            .await;

        for (protocol_semantic_version, circuit_prover_stats) in stats {
            for (tuple, stat) in circuit_prover_stats {
                let CircuitIdRoundTuple {
                    circuit_id,
                    aggregation_round,
                } = tuple;
                let JobCountStatistics {
                    queued,
                    in_progress,
                } = stat;
                let group_id = self
                    .config
                    .get_group_id_for_circuit_id_and_aggregation_round(
                        circuit_id,
                        aggregation_round,
                    )
                    .unwrap_or(u8::MAX);

                FRI_PROVER_METRICS.report_prover_jobs(
                    "queued",
                    circuit_id,
                    aggregation_round,
                    group_id,
                    protocol_semantic_version,
                    queued as u64,
                );

                FRI_PROVER_METRICS.report_prover_jobs(
                    "in_progress",
                    circuit_id,
                    aggregation_round,
                    group_id,
                    protocol_semantic_version,
                    in_progress as u64,
                )
            }
        }

        let lag_by_circuit_type = connection
            .fri_prover_jobs_dal()
            .min_unproved_l1_batch_number()
            .await;

        for ((circuit_id, aggregation_round), l1_batch_number) in lag_by_circuit_type {
            FRI_PROVER_METRICS.block_number
                [&(circuit_id.to_string(), aggregation_round.to_string())]
                .set(l1_batch_number.0 as u64);
        }

        Ok(())
    }
}
