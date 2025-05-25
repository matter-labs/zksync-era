use std::time::Instant;

use async_trait::async_trait;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_circuit_prover_service::types::circuit_wrapper::CircuitWrapper;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{keys::FriCircuitKey, FriProofWrapper};
use zksync_types::basic_fri_types::AggregationRound;

use crate::{
    artifacts::{ArtifactsManager, JobId},
    rounds::scheduler::{Scheduler, SchedulerArtifacts},
};

#[async_trait]
impl ArtifactsManager for Scheduler {
    type InputMetadata = JobId;
    type InputArtifacts = FriProofWrapper;
    type OutputArtifacts = SchedulerArtifacts;
    type BlobUrls = String;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let key = *metadata;
        let artifacts = object_store.get((key.id(), key.chain_id())).await?;

        Ok(artifacts)
    }

    async fn save_to_bucket(
        job_id: JobId,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> String {
        let key = FriCircuitKey {
            batch_id: job_id.into(),
            circuit_id: 1,
            sequence_number: 0,
            depth: 0,
            aggregation_round: AggregationRound::Scheduler,
        };

        object_store
            .put(
                key,
                &CircuitWrapper::Recursive(artifacts.scheduler_circuit.clone()),
            )
            .await
            .unwrap()
    }

    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: JobId,
        started_at: Instant,
        blob_urls: String,
        _artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        let mut prover_connection = connection_pool.connection().await?;
        let mut transaction = prover_connection.start_transaction().await?;
        let protocol_version_id = transaction
            .fri_basic_witness_generator_dal()
            .protocol_version_for_l1_batch(job_id.into())
            .await
            .unwrap();
        let batch_sealed_at = transaction
            .fri_basic_witness_generator_dal()
            .get_batch_sealed_at_timestamp(job_id.into())
            .await;

        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                job_id.into(),
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
                0,
                0,
                AggregationRound::Scheduler,
                &blob_urls,
                false,
                protocol_version_id,
                batch_sealed_at,
            )
            .await;

        transaction
            .fri_scheduler_witness_generator_dal()
            .mark_scheduler_job_as_successful(job_id.into(), started_at.elapsed())
            .await;

        transaction.commit().await?;
        Ok(())
    }
}
