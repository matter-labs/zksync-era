use std::time::Instant;

use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{keys::FriCircuitKey, CircuitWrapper};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::{
    artifacts::{ArtifactsManager, BlobUrls},
    scheduler::{SchedulerArtifacts, SchedulerWitnessGenerator},
};

impl ArtifactsManager for SchedulerWitnessGenerator {
    type InputMetadata = ();
    type InputArtifacts = ();
    type OutputArtifacts = SchedulerArtifacts;

    async fn get_artifacts(
        metadata: &Self::Medatadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts {
        todo!()
    }

    async fn save_artifacts(
        job_id: u32,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls {
        let key = FriCircuitKey {
            block_number: L1BatchNumber(job_id),
            circuit_id: 1,
            sequence_number: 0,
            depth: 0,
            aggregation_round: AggregationRound::Scheduler,
        };

        let blob_url = object_store
            .put(
                key,
                &CircuitWrapper::Recursive(artifacts.scheduler_circuit.clone()),
            )
            .await?;

        BlobUrls::Url(blob_url)
    }

    async fn update_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: L1BatchNumber,
        started_at: Instant,
        blob_urls: BlobUrls,
        _artifacts: Self::OutputArtifacts,
    ) {
        let blob_url = match blob_urls {
            BlobUrls::Url(url) => url,
            _ => panic!("Unexpected blob urls type"),
        };

        let mut prover_connection = connection_pool.connection().await?;
        let mut transaction = prover_connection.start_transaction().await?;
        let protocol_version_id = transaction
            .fri_witness_generator_dal()
            .protocol_version_for_l1_batch(job_id)
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                job_id,
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
                0,
                0,
                AggregationRound::Scheduler,
                &blob_url,
                false,
                protocol_version_id,
            )
            .await;

        transaction
            .fri_witness_generator_dal()
            .mark_scheduler_job_as_successful(job_id, started_at.elapsed())
            .await;

        transaction.commit().await?;
    }
}
