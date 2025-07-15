use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_job_processor::JobSaver;

use crate::{
    artifact_manager::ArtifactsManager,
    executor::WitnessGeneratorExecutor,
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::{JobManager, JobMetadata},
};

/// Witness Generator job saver implementation.
/// Persists the job execution to database. In case of success, artifacts are uploaded to object store.
#[derive(Debug)]
pub struct WitnessGeneratorJobSaver<R> {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    _marker: std::marker::PhantomData<R>,
}

impl<R> WitnessGeneratorJobSaver<R> {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
    ) -> Self {
        Self {
            connection_pool,
            object_store,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<R> JobSaver for WitnessGeneratorJobSaver<R>
where
    R: JobManager + ArtifactsManager,
{
    type ExecutorType = WitnessGeneratorExecutor<R>;

    #[tracing::instrument(
        name = "witness_generator_job_saver",
        skip_all,
        fields(l1_batch = % data.1.job_id())
    )]
    async fn save_job_result(
        &self,
        data: (anyhow::Result<R::OutputArtifacts>, R::Metadata),
    ) -> anyhow::Result<()> {
        let start_time = Instant::now();
        let (result, metadata) = data;
        let job_id = metadata.job_id();

        tracing::info!(
            "Starting saving witness generation of type {:?} for block {:?}",
            R::ROUND,
            job_id
        );

        match result {
            Ok(artifacts) => {
                let blob_save_started_at = Instant::now();
                let blob_urls =
                    R::save_to_bucket(job_id, artifacts.clone(), &*self.object_store).await;

                WITNESS_GENERATOR_METRICS.blob_save_time[&R::ROUND.into()]
                    .observe(blob_save_started_at.elapsed());

                R::save_to_database(
                    &self.connection_pool,
                    job_id,
                    metadata.started_at(),
                    blob_urls,
                    artifacts,
                )
                .await?;
            }
            Err(error) => {
                let error_message = error.to_string();
                tracing::error!("Witness generator failed: {:?}", error);
                self.connection_pool
                    .connection()
                    .await
                    .unwrap()
                    .fri_witness_generator_dal()
                    .mark_witness_job_failed(
                        &error_message,
                        job_id.id(),
                        job_id.chain_id(),
                        R::ROUND,
                    )
                    .await;
            }
        };
        tracing::info!(
            "Finished saving witness generation of type {:?} for block {:?} in {:?}",
            R::ROUND,
            job_id,
            start_time.elapsed()
        );

        Ok(())
    }
}
