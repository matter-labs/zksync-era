use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_prover_dal::ProverDal;
use zksync_prover_fri_types::get_current_pod_name;
use zksync_queued_job_processor::JobProcessor;

use crate::{
    node_aggregation::{
        prepare_job, NodeAggregationArtifacts, NodeAggregationWitnessGenerator,
        NodeAggregationWitnessGeneratorJob,
    },
    traits::{ArtifactsManager, BlobUrls},
};

#[async_trait]
impl JobProcessor for NodeAggregationWitnessGenerator {
    type Job = NodeAggregationWitnessGeneratorJob;
    type JobId = u32;
    type JobArtifacts = NodeAggregationArtifacts;

    const SERVICE_NAME: &'static str = "fri_node_aggregation_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some(metadata) = prover_connection
            .fri_witness_generator_dal()
            .get_next_node_aggregation_job(self.protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        tracing::info!("Processing node aggregation job {:?}", metadata.id);
        Ok(Some((
            metadata.id,
            prepare_job(metadata, &*self.object_store, self.keystore.clone())
                .await
                .context("prepare_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_node_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<NodeAggregationArtifacts>> {
        let object_store = self.object_store.clone();
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            Ok(Self::process_job_impl(job, started_at, object_store, max_circuits_in_flight).await)
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % artifacts.block_number, circuit_id = % artifacts.circuit_id)
    )]
    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: NodeAggregationArtifacts,
    ) -> anyhow::Result<()> {
        let block_number = artifacts.block_number;
        let circuit_id = artifacts.circuit_id;
        let depth = artifacts.depth;
        let shall_continue_node_aggregations = artifacts.next_aggregations.len() > 1;
        let blob_urls = match Self::save_artifacts(artifacts, &*self.object_store).await {
            BlobUrls::Aggregation(blob_urls) => blob_urls,
            _ => unreachable!(),
        };

        update_database(
            &self.prover_connection_pool,
            started_at,
            job_id,
            block_number,
            depth,
            circuit_id,
            blob_urls,
            shall_continue_node_aggregations,
        )
        .await;
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
            .context("failed to acquire DB connection for NodeAggregationWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_node_aggregation_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for NodeAggregationWitnessGenerator")
    }
}