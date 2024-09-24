use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use zksync_prover_dal::ProverDal;
use zksync_prover_fri_types::{get_current_pod_name, AuxOutputWitnessWrapper};
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::{
    artifacts::ArtifactsManager,
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{
        basic_circuits::{BasicCircuitArtifacts, BasicWitnessGenerator, BasicWitnessGeneratorJob},
        JobManager,
    },
};

#[async_trait]
impl JobProcessor for BasicWitnessGenerator {
    type Job = BasicWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    // The artifact is optional to support skipping blocks when sampling is enabled.
    type JobArtifacts = BasicCircuitArtifacts;

    const SERVICE_NAME: &'static str = "fri_basic_circuit_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();
        let pod_name = get_current_pod_name();
        match prover_connection
            .fri_witness_generator_dal()
            .get_next_basic_circuit_witness_job(
                last_l1_batch_to_process,
                self.protocol_version,
                &pod_name,
            )
            .await
        {
            Some(block_number) => Ok(Some((
                block_number,
                <Self as JobManager>::prepare_job(
                    block_number,
                    &*self.object_store,
                    Keystore::locate(), // todo: this should be removed
                )
                .await?,
            ))),
            None => Ok(None),
        }
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_witness_job_failed(&error, job_id.0, AggregationRound::BasicCircuits)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<BasicCircuitArtifacts>> {
        let object_store = Arc::clone(&self.object_store);
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            <Self as JobManager>::process_job(job, object_store, max_circuits_in_flight, started_at)
                .await
        })
    }

    #[tracing::instrument(skip_all, fields(l1_batch = %job_id))]
    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        optional_artifacts: BasicCircuitArtifacts,
    ) -> anyhow::Result<()> {
        let blob_started_at = Instant::now();

        let aux_output_witness_wrapper =
            AuxOutputWitnessWrapper(optional_artifacts.aux_output_witness.clone());
        if self.config.shall_save_to_public_bucket {
            self.public_blob_store.as_deref()
                        .expect("public_object_store shall not be empty while running with shall_save_to_public_bucket config")
                        .put(job_id, &aux_output_witness_wrapper)
                        .await
                        .unwrap();
        }

        let blob_urls =
            Self::save_to_bucket(job_id.0, optional_artifacts.clone(), &*self.object_store).await;

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::BasicCircuits.into()]
            .observe(blob_started_at.elapsed());

        Self::save_to_database(
            &self.prover_connection_pool,
            job_id.0,
            started_at,
            blob_urls,
            optional_artifacts,
        )
        .await?;
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .prover_connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for BasicWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_witness_job_attempts(job_id.0, AggregationRound::BasicCircuits)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for BasicWitnessGenerator")
    }
}
