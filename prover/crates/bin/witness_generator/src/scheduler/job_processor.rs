use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_prover_dal::ProverDal;
use zksync_prover_fri_types::get_current_pod_name;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::{
    artifacts::ArtifactsManager,
    metrics::WITNESS_GENERATOR_METRICS,
    scheduler::{
        prepare_job, SchedulerArtifacts, SchedulerWitnessGenerator, SchedulerWitnessGeneratorJob,
    },
};

#[async_trait]
impl JobProcessor for SchedulerWitnessGenerator {
    type Job = SchedulerWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = SchedulerArtifacts;

    const SERVICE_NAME: &'static str = "fri_scheduler_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some(l1_batch_number) = prover_connection
            .fri_witness_generator_dal()
            .get_next_scheduler_witness_job(self.protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        let recursion_tip_job_id = prover_connection
            .fri_prover_jobs_dal()
            .get_recursion_tip_proof_job_id(l1_batch_number)
            .await
            .context(format!(
                "could not find recursion tip proof for l1 batch {}",
                l1_batch_number
            ))?;

        Ok(Some((
            l1_batch_number,
            prepare_job(
                l1_batch_number,
                recursion_tip_job_id,
                &*self.object_store,
                self.keystore.clone(),
            )
            .await
            .context("prepare_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_scheduler_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<SchedulerArtifacts>> {
        tokio::task::spawn_blocking(move || {
            let block_number = job.block_number;
            let _span = tracing::info_span!("scheduler", %block_number).entered();
            Ok(Self::process_job_sync(job, started_at))
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %job_id)
    )]
    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: SchedulerArtifacts,
    ) -> anyhow::Result<()> {
        let blob_save_started_at = Instant::now();

        let blob_urls =
            Self::save_artifacts(job_id.0, artifacts.clone(), &*self.object_store).await;

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::Scheduler.into()]
            .observe(blob_save_started_at.elapsed());

        Self::update_database(
            &self.prover_connection_pool,
            job_id.0,
            started_at,
            blob_urls,
            artifacts,
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
            .context("failed to acquire DB connection for SchedulerWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_scheduler_witness_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for SchedulerWitnessGenerator")
    }
}
