use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::AuxOutputWitnessWrapper;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_prover_interface::{inputs::WitnessInputData, Bincode};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchId};

use crate::{
    artifact_manager::{ArtifactsManager, JobId},
    rounds::basic_circuits::{
        utils::create_aggregation_jobs, BasicCircuitArtifacts, BasicCircuits,
        BasicWitnessGeneratorJob,
    },
    utils::SchedulerPartialInputWrapper,
};

#[async_trait]
impl ArtifactsManager for BasicCircuits {
    type InputMetadata = L1BatchId;
    type InputArtifacts = BasicWitnessGeneratorJob;
    type OutputArtifacts = BasicCircuitArtifacts;
    type BlobUrls = String;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let batch_id = *metadata;
        let data: WitnessInputData = match object_store.get(batch_id).await {
            Ok(data) => data,
            Err(err_cbor) => object_store
                .get::<WitnessInputData<Bincode>>(batch_id)
                .await
                .map(Into::into)
                .map_err(|err_bincode| anyhow::anyhow!("Getting data with bincode failed with {err_bincode}, getting data with CBOR failed with {err_cbor}"))?,
        };

        Ok(BasicWitnessGeneratorJob { batch_id, data })
    }

    async fn save_to_bucket(
        job_id: JobId,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> String {
        let aux_output_witness_wrapper =
            AuxOutputWitnessWrapper(artifacts.aux_output_witness.clone());

        object_store
            .put(job_id.into(), &aux_output_witness_wrapper)
            .await
            .unwrap();
        let wrapper = SchedulerPartialInputWrapper(artifacts.scheduler_witness);
        object_store.put(job_id.into(), &wrapper).await.unwrap()
    }

    #[tracing::instrument(skip_all, fields(l1_batch = %job_id))]
    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: JobId,
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
            .protocol_version_for_l1_batch(job_id.into())
            .await
            .unwrap();
        let batch_sealed_at = transaction
            .fri_basic_witness_generator_dal()
            .get_batch_sealed_at_timestamp(job_id.into())
            .await;

        transaction
            .fri_prover_jobs_dal()
            .insert_prover_jobs(
                job_id.into(),
                artifacts.circuit_urls,
                AggregationRound::BasicCircuits,
                0,
                protocol_version_id,
                batch_sealed_at,
            )
            .await;

        create_aggregation_jobs(
            &mut transaction,
            job_id.into(),
            &artifacts.queue_urls,
            &blob_urls,
            get_recursive_layer_circuit_id_for_base_layer,
            protocol_version_id,
        )
        .await
        .unwrap();

        transaction
            .fri_basic_witness_generator_dal()
            .mark_witness_job_as_successful(job_id.into(), started_at.elapsed())
            .await;
        transaction
            .commit()
            .await
            .expect("failed to commit database transaction");
        Ok(())
    }
}
