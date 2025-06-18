use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use circuit_definitions::circuit_definitions::{
    base_layer::ZkSyncBaseLayerVerificationKey,
    recursion_layer::ZkSyncRecursionLayerVerificationKey,
};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, H256,
};

use crate::artifact_manager::{ArtifactsManager, JobId};

mod basic_circuits;
mod leaf_aggregation;
mod node_aggregation;
mod recursion_tip;
mod scheduler;

pub use basic_circuits::BasicCircuits;
pub use leaf_aggregation::LeafAggregation;
pub use node_aggregation::NodeAggregation;
pub use recursion_tip::RecursionTip;
pub use scheduler::Scheduler;

pub trait JobMetadata {
    fn job_id(&self) -> JobId;
    fn started_at(&self) -> Instant;
}

#[async_trait]
pub trait JobManager: ArtifactsManager + Sync + Send + 'static {
    type Job: Send + 'static;
    type Metadata: Clone + Send + JobMetadata + 'static;

    const ROUND: AggregationRound;
    const SERVICE_NAME: &'static str;

    async fn process_job(
        job: Self::Job,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> anyhow::Result<Self::OutputArtifacts>;

    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Arc<dyn VerificationKeyManager>,
    ) -> anyhow::Result<Self::Job>;

    async fn get_metadata(
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<Self::Metadata>>;
}

pub trait VerificationKeyManager: 'static + Send + Sync {
    fn load_base_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncBaseLayerVerificationKey>;
    fn load_recursive_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncRecursionLayerVerificationKey>;
    fn verify_scheduler_vk_hash(&self, expected_hash: H256) -> anyhow::Result<()>;

    fn get_leaf_vk_params(
        &self,
    ) -> anyhow::Result<Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>>;
}

// TODO: implement SimpleKeystore for VerificationKeyManager
