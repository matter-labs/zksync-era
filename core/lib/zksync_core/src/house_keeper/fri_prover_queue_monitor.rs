use async_trait::async_trait;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_dal::ConnectionPool;
use zksync_prover_utils::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct FriProverStatsReporter {
    reporting_interval_ms: u64,
    prover_connection_pool: ConnectionPool,
    db_connection_pool: ConnectionPool,
    config: FriProverGroupConfig,
}

impl FriProverStatsReporter {
    pub fn new(
        reporting_interval_ms: u64,
        prover_connection_pool: ConnectionPool,
        db_connection_pool: ConnectionPool,
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

///  Invoked periodically to push prover queued/in-progress job statistics
#[async_trait]
impl PeriodicJob for FriProverStatsReporter {
    const SERVICE_NAME: &'static str = "FriProverStatsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.prover_connection_pool.access_storage().await.unwrap();
        let stats = conn.fri_prover_jobs_dal().get_prover_jobs_stats().await;

        for ((circuit_id, aggregation_round), stats) in stats.into_iter() {
            // BEWARE, HERE BE DRAGONS.
            // In database, the circuit_id stored is the circuit for which the aggregation is done,
            // not the circuit which is running.
            // There is a single node level aggregation circuit, which is circuit 2.
            // This can aggregate multiple leaf nodes (which may belong to different circuits).
            // This reporting is a hacky forced way to use circuit_id 2 which will solve autoscalers.
            // A proper fix will be later provided to solve this at database level.
            let circuit_id = if aggregation_round == 2 {
                2
            } else {
                circuit_id
            };

            let group_id = self
                .config
                .get_group_id_for_circuit_id_and_aggregation_round(circuit_id, aggregation_round)
                .unwrap_or(u8::MAX);

            metrics::gauge!(
              "fri_prover.prover.jobs",
              stats.queued as f64,
              "type" => "queued",
              "circuit_id" => circuit_id.to_string(),
              "aggregation_round" => aggregation_round.to_string(),
              "prover_group_id" => group_id.to_string(),
            );

            metrics::gauge!(
              "fri_prover.prover.jobs",
              stats.in_progress as f64,
              "type" => "in_progress",
              "circuit_id" => circuit_id.to_string(),
              "aggregation_round" => aggregation_round.to_string(),
              "prover_group_id" => group_id.to_string(),
            );
        }

        let lag_by_circuit_type = conn
            .fri_prover_jobs_dal()
            .min_unproved_l1_batch_number()
            .await;

        for ((circuit_id, aggregation_round), l1_batch_number) in lag_by_circuit_type {
            metrics::gauge!(
              "fri_prover.block_number", l1_batch_number.0 as f64,
              "circuit_id" => circuit_id.to_string(),
              "aggregation_round" => aggregation_round.to_string());
        }

        // FIXME: refactor metrics here

        let mut db_conn = self.db_connection_pool.access_storage().await.unwrap();

        if let Some(l1_batch_number) = db_conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
        {
            metrics::gauge!("fri_prover.oldest_unpicked_batch", l1_batch_number.0 as f64)
        }

        if let Some(l1_batch_number) = db_conn
            .proof_generation_dal()
            .get_oldest_not_generated_batch()
            .await
        {
            metrics::gauge!(
                "fri_prover.oldest_not_generated_batch",
                l1_batch_number.0 as f64
            )
        }

        for aggregation_round in 0..3 {
            if let Some(l1_batch_number) = conn
                .fri_prover_jobs_dal()
                .min_unproved_l1_batch_number_for_aggregation_round(aggregation_round.into())
                .await
            {
                metrics::gauge!(
                    "fri_prover.oldest_unprocessed_block_by_round",
                    l1_batch_number.0 as f64,
                    "aggregation_round" => aggregation_round.to_string()
                )
            }
        }

        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.reporting_interval_ms
    }
}
