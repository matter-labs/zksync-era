use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};

#[derive(Debug)]
pub struct AggregationBlobUrls {
    pub aggregation_urls: String,
    pub circuit_ids_and_urls: Vec<(u8, String)>,
}

#[async_trait]
pub trait ArtifactsManager {
    type InputMetadata;
    type InputArtifacts;
    type OutputArtifacts: Send + Clone + 'static;
    type BlobUrls;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts>;

    async fn save_to_bucket(
        job_id: u32,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
        shall_save_to_public_bucket: bool,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
    ) -> Self::BlobUrls;

    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
        started_at: Instant,
        blob_urls: Self::BlobUrls,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()>;
}
