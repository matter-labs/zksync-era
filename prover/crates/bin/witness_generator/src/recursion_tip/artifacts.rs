use std::time::Instant;

use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::{
    recursion_tip::{RecursionTipArtifacts, RecursionTipWitnessGenerator},
    traits::{ArtifactsManager, BlobUrls},
};

impl ArtifactsManager for RecursionTipWitnessGenerator {
    type InputMetadata = ();
    type InputArtifacts = ();
    type OutputArtifacts = RecursionTipArtifacts;

    async fn get_artifacts(
        metadata: &Self::Medatadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts {
        todo!()
    }

    async fn save_artifacts(
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls {
        todo!()
    }

    async fn update_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
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
            .protocol_version_for_l1_batch(L1BatchNumber(job_id))
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                L1BatchNumber(job_id),
                ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8,
                0,
                0,
                AggregationRound::RecursionTip,
                &blob_url,
                false,
                protocol_version_id,
            )
            .await;

        transaction
            .fri_witness_generator_dal()
            .mark_recursion_tip_job_as_successful(L1BatchNumber(job_id), started_at.elapsed())
            .await;

        transaction.commit().await?;
    }
}
