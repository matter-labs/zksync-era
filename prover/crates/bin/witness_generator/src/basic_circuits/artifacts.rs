use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::AuxOutputWitnessWrapper;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::{
    artifacts::{ArtifactsManager, BlobUrls},
    basic_circuits::{BasicCircuitArtifacts, BasicWitnessGenerator, BasicWitnessGeneratorJob},
    utils::SchedulerPartialInputWrapper,
};

#[async_trait]
impl ArtifactsManager for BasicWitnessGenerator {
    type InputMetadata = L1BatchNumber;
    type InputArtifacts = BasicWitnessGeneratorJob;
    type OutputArtifacts = BasicCircuitArtifacts;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let l1_batch_number = *metadata;
        let data = object_store.get(l1_batch_number).await.unwrap();
        Ok(BasicWitnessGeneratorJob {
            block_number: l1_batch_number,
            data,
        })
    }

    async fn save_artifacts(
        job_id: u32,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls {
        let aux_output_witness_wrapper = AuxOutputWitnessWrapper(artifacts.aux_output_witness);
        object_store
            .put(L1BatchNumber(job_id), &aux_output_witness_wrapper)
            .await
            .unwrap();
        let wrapper = SchedulerPartialInputWrapper(artifacts.scheduler_witness);
        let url = object_store
            .put(L1BatchNumber(job_id), &wrapper)
            .await
            .unwrap();

        BlobUrls::Url(url)
    }

    #[tracing::instrument(skip_all, fields(l1_batch = %job_id))]
    async fn update_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
        started_at: Instant,
        blob_urls: BlobUrls,
        _artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        let blob_urls = match blob_urls {
            BlobUrls::Scheduler(blobs) => blobs,
            _ => unreachable!(),
        };

        let mut connection = connection_pool
            .connection()
            .await
            .expect("failed to get database connection");
        let mut transaction = connection
            .start_transaction()
            .await
            .expect("failed to get database transaction");
        let protocol_version_id = transaction
            .fri_witness_generator_dal()
            .protocol_version_for_l1_batch(L1BatchNumber(job_id))
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_jobs(
                L1BatchNumber(job_id),
                blob_urls.circuit_ids_and_urls,
                AggregationRound::BasicCircuits,
                0,
                protocol_version_id,
            )
            .await;
        transaction
            .fri_witness_generator_dal()
            .create_aggregation_jobs(
                L1BatchNumber(job_id),
                &blob_urls.closed_form_inputs_and_urls,
                &blob_urls.scheduler_witness_url,
                get_recursive_layer_circuit_id_for_base_layer,
                protocol_version_id,
            )
            .await;
        transaction
            .fri_witness_generator_dal()
            .mark_witness_job_as_successful(L1BatchNumber(job_id), started_at.elapsed())
            .await;
        transaction
            .commit()
            .await
            .expect("failed to commit database transaction");
        Ok(())
    }
}
