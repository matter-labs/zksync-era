use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use tracing::Instrument;
use zksync_prover_dal::ProverDal;
use zksync_prover_fri_types::get_current_pod_name;
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::{
    basic_circuits::{
        artifacts::SchedulerArtifacts, update_database, BasicCircuitArtifacts,
        BasicWitnessGenerator, BasicWitnessGeneratorJob,
    },
    traits::{ArtifactsManager, BlobUrls},
};

#[async_trait]
impl JobProcessor for BasicWitnessGenerator {
    type Job = BasicWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    // The artifact is optional to support skipping blocks when sampling is enabled.
    type JobArtifacts = Option<BasicCircuitArtifacts>;

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
            Some(block_number) => {
                tracing::info!(
                    "Processing FRI basic witness-gen for block {}",
                    block_number
                );
                let started_at = Instant::now();
                let job = Self::get_artifacts(&block_number, &*self.object_store).await;

                crate::metrics::WITNESS_GENERATOR_METRICS.blob_fetch_time
                    [&AggregationRound::BasicCircuits.into()]
                    .observe(started_at.elapsed());

                Ok(Some((block_number, job)))
            }
            None => Ok(None),
        }
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_witness_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: BasicWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<Option<BasicCircuitArtifacts>>> {
        let object_store = Arc::clone(&self.object_store);
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            let block_number = job.block_number;
            Ok(
                Self::process_job_impl(object_store, job, started_at, max_circuits_in_flight)
                    .instrument(tracing::info_span!("basic_circuit", %block_number))
                    .await,
            )
        })
    }

    #[tracing::instrument(skip_all, fields(l1_batch = %job_id))]
    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        optional_artifacts: Option<BasicCircuitArtifacts>,
    ) -> anyhow::Result<()> {
        match optional_artifacts {
            None => Ok(()),
            Some(artifacts) => {
                let blob_started_at = Instant::now();
                let scheduler_witness_url = match Self::save_artifacts(
                    SchedulerArtifacts {
                        block_number: job_id,
                        scheduler_partial_input: artifacts.scheduler_witness,
                        aux_output_witness: artifacts.aux_output_witness,
                        public_object_store: self.public_blob_store.as_deref(),
                        shall_save_to_public_bucket: self.config.shall_save_to_public_bucket,
                    },
                    &*self.object_store,
                )
                .await
                {
                    BlobUrls::Url(url) => url,
                    _ => unreachable!(),
                };

                crate::metrics::WITNESS_GENERATOR_METRICS.blob_save_time
                    [&AggregationRound::BasicCircuits.into()]
                    .observe(blob_started_at.elapsed());

                update_database(
                    &self.prover_connection_pool,
                    started_at,
                    job_id,
                    BlobUrls {
                        circuit_ids_and_urls: artifacts.circuit_urls,
                        closed_form_inputs_and_urls: artifacts.queue_urls,
                        scheduler_witness_url,
                    },
                )
                .await;
                Ok(())
            }
        }
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
            .get_basic_circuit_witness_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for BasicWitnessGenerator")
    }
}