use std::{marker::PhantomData, sync::Arc, time::Instant};

use async_trait::async_trait;
use tokio::runtime::Handle;
use zksync_object_store::ObjectStore;
use zksync_prover_job_processor::Executor;

use crate::metrics::WITNESS_GENERATOR_METRICS;
use crate::rounds::{JobManager, JobMetadata};
use crate::artifact_manager::ArtifactsManager;

/// WitnessGenerator executor implementation.
pub struct WitnessGeneratorExecutor<R> 
where
    R: JobManager + ArtifactsManager,
{
    object_store: Arc<dyn ObjectStore>,
    max_circuits_in_flight: usize,
    _marker: PhantomData<R>,
}

impl<R> WitnessGeneratorExecutor<R> 
where
    R: JobManager + ArtifactsManager,
{
    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> Self {
        Self {
            object_store,
            max_circuits_in_flight,
            _marker: PhantomData,
        }
    }
}

#[async_trait]
impl<R> Executor for WitnessGeneratorExecutor<R> 
where
    R: JobManager + ArtifactsManager,
{
    type Input = R::Job;
    type Output = R::OutputArtifacts;
    type Metadata = R::Metadata;

    #[tracing::instrument(
        name = "witness_generator_executor",
        skip_all,
        fields(l1_batch = % metadata.job_id())
    )]
    fn execute(
        &self,
        data: Self::Input,
        metadata: Self::Metadata,
    ) -> anyhow::Result<Self::Output> {
        let started_at = Instant::now();

        tracing::info!(
            "Starting executing witness generation of type {:?} for block {:?}",
            R::ROUND,
            metadata.job_id()
        );

        let object_store = self.object_store.clone();
        let max_circuits_in_flight = self.max_circuits_in_flight;
        let rt_handle = Handle::current();
        let artifacts = rt_handle.block_on(
            R::process_job(data, object_store, max_circuits_in_flight)
        );

        WITNESS_GENERATOR_METRICS.witness_generation_time
            [&R::ROUND.into()]
            .observe(started_at.elapsed());
        tracing::info!(
            "Finished executing witness generation of type {:?} for block {:?} in {:?}",
            R::ROUND,
            metadata.job_id(),
            started_at.elapsed()
        );

        artifacts
    }
}
