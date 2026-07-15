use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_types::{L1BatchId, L1BatchNumber, L2ChainId};

#[derive(Debug)]
pub struct AggregationBlobUrls {
    pub aggregation_urls: String,
    pub circuit_ids_sequence_numbers_and_urls: Vec<(u8, usize, String)>,
}

#[derive(Debug, Clone, Copy)]
pub struct JobId {
    pub id: u32,
    pub chain_id: L2ChainId,
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.id, self.chain_id)
    }
}

impl From<L1BatchId> for JobId {
    fn from(batch_id: L1BatchId) -> Self {
        Self::new(batch_id.batch_number().0, batch_id.chain_id())
    }
}

impl From<JobId> for L1BatchId {
    fn from(job_id: JobId) -> Self {
        L1BatchId::new(job_id.chain_id, L1BatchNumber(job_id.id))
    }
}

impl JobId {
    pub fn new(id: u32, chain_id: L2ChainId) -> Self {
        Self { id, chain_id }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn chain_id(&self) -> L2ChainId {
        self.chain_id
    }
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
        job_id: JobId,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> Self::BlobUrls;

    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: JobId,
        started_at: Instant,
        blob_urls: Self::BlobUrls,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()>;
}
