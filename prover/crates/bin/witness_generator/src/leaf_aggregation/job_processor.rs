use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_prover_dal::ProverDal;
use zksync_prover_fri_types::get_current_pod_name;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::basic_fri_types::AggregationRound;

use crate::{
    artifacts::ArtifactsManager,
    leaf_aggregation::{
        prepare_leaf_aggregation_job, LeafAggregationArtifacts, LeafAggregationWitnessGenerator,
        LeafAggregationWitnessGeneratorJob,
    },
    metrics::WITNESS_GENERATOR_METRICS,
};

#[async_trait]
impl JobProcessor for LeafAggregationWitnessGenerator {
    type Job = LeafAggregationWitnessGeneratorJob;
    type JobId = u32;
    type JobArtifacts = LeafAggregationArtifacts;

    const SERVICE_NAME: &'static str = "fri_leaf_aggregation_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some(metadata) = prover_connection
            .fri_witness_generator_dal()
            .get_next_leaf_aggregation_job(self.protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        tracing::info!("Processing leaf aggregation job {:?}", metadata.id);
        Ok(Some((
            metadata.id,
            prepare_leaf_aggregation_job(metadata, &*self.object_store, self.keystore.clone())
                .await
                .context("prepare_leaf_aggregation_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_leaf_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<LeafAggregationArtifacts>> {
        let object_store = self.object_store.clone();
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            Ok(Self::process_job_impl(job, started_at, object_store, max_circuits_in_flight).await)
        })
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: LeafAggregationArtifacts,
    ) -> anyhow::Result<()> {
        let block_number = artifacts.block_number;
        let circuit_id = artifacts.circuit_id;
        tracing::info!(
            "Saving leaf aggregation artifacts for block {} with circuit {}",
            block_number.0,
            circuit_id,
        );

        let blob_save_started_at = Instant::now();

        let blob_urls = Self::save_artifacts(job_id, artifacts.clone(), &*self.object_store).await;

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::LeafAggregation.into()]
            .observe(blob_save_started_at.elapsed());

        tracing::info!(
            "Saved leaf aggregation artifacts for block {} with circuit {}",
            block_number.0,
            circuit_id,
        );
        Self::update_database(
            &self.prover_connection_pool,
            job_id,
            started_at,
            blob_urls,
            artifacts,
        )
        .await?;
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .prover_connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for LeafAggregationWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_leaf_aggregation_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for LeafAggregationWitnessGenerator")
    }
}
