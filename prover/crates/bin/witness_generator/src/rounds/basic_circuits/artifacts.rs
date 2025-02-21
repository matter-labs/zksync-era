use std::{iter::chain, sync::Arc, time::Instant};

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::AuxOutputWitnessWrapper;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber, L2ChainId};

use crate::{
    artifacts::ArtifactsManager,
    rounds::basic_circuits::{
        utils::create_aggregation_jobs, BasicCircuitArtifacts, BasicCircuits,
        BasicWitnessGeneratorJob,
    },
    utils::SchedulerPartialInputWrapper,
};

#[async_trait]
impl ArtifactsManager for BasicCircuits {
    type InputMetadata = (L2ChainId, L1BatchNumber);
    type InputArtifacts = BasicWitnessGeneratorJob;
    type OutputArtifacts = BasicCircuitArtifacts;
    type BlobUrls = String;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let (chain_id, l1_batch_number) = *metadata;
        let data = object_store.get((chain_id, l1_batch_number)).await.unwrap();
        Ok(BasicWitnessGeneratorJob {
            chain_id,
            block_number: l1_batch_number,
            data,
        })
    }

    async fn save_to_bucket(
        job_id: u32,
        chain_id: L2ChainId,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
        shall_save_to_public_bucket: bool,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
    ) -> String {
        let aux_output_witness_wrapper =
            AuxOutputWitnessWrapper(artifacts.aux_output_witness.clone());
        if shall_save_to_public_bucket {
            public_blob_store.as_deref()
                .expect("public_object_store shall not be empty while running with shall_save_to_public_bucket config")
                .put((chain_id, L1BatchNumber(job_id)), &aux_output_witness_wrapper)
                .await
                .unwrap();
        }

        object_store
            .put(
                (chain_id, L1BatchNumber(job_id)),
                &aux_output_witness_wrapper,
            )
            .await
            .unwrap();
        let wrapper = SchedulerPartialInputWrapper(artifacts.scheduler_witness);
        object_store
            .put((chain_id, L1BatchNumber(job_id)), &wrapper)
            .await
            .unwrap()
    }

    #[tracing::instrument(skip_all, fields(l1_batch = %job_id))]
    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
        chain_id: L2ChainId,
        started_at: Instant,
        blob_urls: String,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        let mut connection = connection_pool
            .connection()
            .await
            .expect("failed to get database connection");
        let mut transaction = connection
            .start_transaction()
            .await
            .expect("failed to get database transaction");
        let protocol_version_id = transaction
            .fri_basic_witness_generator_dal()
            .protocol_version_for_l1_batch_and_chain(L1BatchNumber(job_id), chain_id)
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_jobs(
                L1BatchNumber(job_id),
                chain_id,
                artifacts.circuit_urls,
                AggregationRound::BasicCircuits,
                0,
                protocol_version_id,
            )
            .await;

        create_aggregation_jobs(
            &mut transaction,
            L1BatchNumber(job_id),
            chain_id,
            &artifacts.queue_urls,
            &blob_urls,
            get_recursive_layer_circuit_id_for_base_layer,
            protocol_version_id,
        )
        .await
        .unwrap();

        transaction
            .fri_basic_witness_generator_dal()
            .mark_witness_job_as_successful(L1BatchNumber(job_id), chain_id, started_at.elapsed())
            .await;
        transaction
            .commit()
            .await
            .expect("failed to commit database transaction");
        Ok(())
    }
}
