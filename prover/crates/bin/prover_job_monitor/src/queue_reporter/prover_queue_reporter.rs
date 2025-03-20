use async_trait::async_trait;
use zksync_prover_dal::{Connection, Prover, ProverDal};
use zksync_types::{basic_fri_types::CircuitIdRoundTuple, prover_dal::JobCountStatistics};

use crate::{metrics::FRI_PROVER_METRICS, task_wiring::Task};

/// `ProverQueueReporter` is a task that reports prover jobs status.
/// Note: these values will be used for auto-scaling provers and Witness Vector Generators.
#[derive(Debug)]
pub struct ProverQueueReporter;

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

                FRI_PROVER_METRICS.report_prover_jobs(
                    "queued",
                    circuit_id,
                    aggregation_round,
                    protocol_semantic_version,
                    queued as u64,
                );

                FRI_PROVER_METRICS.report_prover_jobs(
                    "in_progress",
                    circuit_id,
                    aggregation_round,
                    protocol_semantic_version,
                    in_progress as u64,
                )
            }
        }

        let lag_by_circuit_type = connection
            .fri_prover_jobs_dal()
            .min_unproved_l1_batch_number()
            .await;

        // todo: this metric might be revisited
        for ((circuit_id, aggregation_round), batch_number) in lag_by_circuit_type {
            FRI_PROVER_METRICS.block_number
                [&(circuit_id.to_string(), aggregation_round.to_string())]
                .set(batch_number.raw_batch_number() as u64);
        }

        Ok(())
    }
}
