use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};

#[derive(Debug)]
pub(crate) struct AggregationBlobUrls {
    pub aggregations_urls: String,
    pub circuit_ids_and_urls: Vec<(u8, String)>,
}

#[derive(Debug)]
pub(crate) struct SchedulerBlobUrls {
    pub circuit_ids_and_urls: Vec<(u8, String)>,
    pub closed_form_inputs_and_urls: Vec<(u8, String, usize)>,
    pub scheduler_witness_url: String,
}

pub(crate) enum BlobUrls {
    Url(String),
    Aggregation(AggregationBlobUrls),
    Scheduler(SchedulerBlobUrls),
}

#[async_trait]
pub(crate) trait ArtifactsManager {
    type InputMetadata;
    type InputArtifacts;
    type OutputArtifacts;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts>;

    async fn save_artifacts(
        job_id: u32,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls;

    async fn update_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
        started_at: Instant,
        blob_urls: BlobUrls,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()>;
}
