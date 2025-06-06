use std::collections::HashMap;

use anyhow::Context;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;
use zksync_types::{basic_fri_types::CircuitIdRoundTuple, prover_dal::JobCountStatistics};

use crate::metrics::{ProverJobsLabels, FRI_PROVER_METRICS};

/// `ProverQueueReporter` is a task that reports prover jobs status.
/// Note: these values will be used for auto-scaling provers and Witness Vector Generators.
#[derive(Debug)]
pub struct ProverQueueReporter {
    pool: ConnectionPool<Prover>,
}

impl ProverQueueReporter {
    pub fn new(pool: ConnectionPool<Prover>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Task for ProverQueueReporter {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        let stats = connection
            .fri_prover_jobs_dal()
            .get_prover_jobs_stats()
            .await;

        let mut prover_jobs_metric: HashMap<_, _> =
            FRI_PROVER_METRICS.prover_jobs.to_entries().collect();
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

                ["queued", "in_progress"].iter().for_each(|t| {
                    prover_jobs_metric.remove(&ProverJobsLabels {
                        r#type: t,
                        circuit_id: circuit_id.to_string(),
                        aggregation_round: aggregation_round.to_string(),
                        protocol_version: protocol_semantic_version.to_string(),
                    });
                });

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

        // Clean up all unset in this round metrics.
        prover_jobs_metric.iter().for_each(|(k, _)| {
            FRI_PROVER_METRICS.prover_jobs[k].set(0);
        });

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
