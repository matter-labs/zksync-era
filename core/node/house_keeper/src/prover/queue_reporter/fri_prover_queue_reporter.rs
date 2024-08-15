use async_trait::async_trait;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{Prover, ProverDal};
use zksync_types::{basic_fri_types::CircuitIdRoundTuple, prover_dal::JobCountStatistics};

use crate::prover::metrics::FRI_PROVER_METRICS;

/// `FriProverQueueReporter` is a task that periodically reports prover jobs status.
/// Note: these values will be used for auto-scaling provers and Witness Vector Generators.
#[derive(Debug)]
pub struct FriProverQueueReporter {
    reporting_interval_ms: u64,
    prover_connection_pool: ConnectionPool<Prover>,
    db_connection_pool: ConnectionPool<Core>,
    config: FriProverGroupConfig,
}

impl FriProverQueueReporter {
    pub fn new(
        reporting_interval_ms: u64,
        prover_connection_pool: ConnectionPool<Prover>,
        db_connection_pool: ConnectionPool<Core>,
        config: FriProverGroupConfig,
    ) -> Self {
        Self {
            reporting_interval_ms,
            prover_connection_pool,
            db_connection_pool,
            config,
        }
    }
}

#[async_trait]
impl PeriodicJob for FriProverQueueReporter {
    const SERVICE_NAME: &'static str = "FriProverQueueReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.prover_connection_pool.connection().await.unwrap();
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
                );
            }
        }

        let lag_by_circuit_type = conn
            .fri_prover_jobs_dal()
            .min_unproved_l1_batch_number()
            .await;

        for ((circuit_id, aggregation_round), l1_batch_number) in lag_by_circuit_type {
            FRI_PROVER_METRICS.block_number
                [&(circuit_id.to_string(), aggregation_round.to_string())]
                .set(l1_batch_number.0 as u64);
        }

        // FIXME: refactor metrics here

        let mut db_conn = self.db_connection_pool.connection().await.unwrap();

        let oldest_unpicked_batch = match db_conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await?
        {
            Some(l1_batch_number) => l1_batch_number.0 as u64,
            // if there is no unpicked batch in database, we use sealed batch number as a result
            None => {
                db_conn
                    .blocks_dal()
                    .get_sealed_l1_batch_number()
                    .await
                    .unwrap()
                    .unwrap()
                    .0 as u64
            }
        };
        FRI_PROVER_METRICS
            .oldest_unpicked_batch
            .set(oldest_unpicked_batch);

        if let Some(l1_batch_number) = db_conn
            .proof_generation_dal()
            .get_oldest_not_generated_batch()
            .await?
        {
            FRI_PROVER_METRICS
                .oldest_not_generated_batch
                .set(l1_batch_number.0 as u64);
        }

        for aggregation_round in 0..3 {
            if let Some(l1_batch_number) = conn
                .fri_prover_jobs_dal()
                .min_unproved_l1_batch_number_for_aggregation_round(aggregation_round.into())
                .await
            {
                FRI_PROVER_METRICS.oldest_unprocessed_block_by_round
                    [&aggregation_round.to_string()]
                    .set(l1_batch_number.0 as u64);
            }
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
