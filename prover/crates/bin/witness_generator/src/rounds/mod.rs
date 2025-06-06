use std::{marker::PhantomData, sync::Arc, time::Instant};

use anyhow::Context;
use async_trait::async_trait;
use tokio::task::JoinHandle;
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::artifacts::{ArtifactsManager, JobId};

mod basic_circuits;
mod leaf_aggregation;
mod node_aggregation;
mod recursion_tip;
mod scheduler;

pub use basic_circuits::BasicCircuits;
pub use leaf_aggregation::LeafAggregation;
pub use node_aggregation::NodeAggregation;
pub use recursion_tip::RecursionTip;
pub use scheduler::Scheduler;
use zksync_types::basic_fri_types::AggregationRound;

use crate::metrics::WITNESS_GENERATOR_METRICS;

pub trait JobMetadata {
    fn job_id(&self) -> JobId;
}

#[async_trait]
pub trait JobManager: ArtifactsManager {
    type Job: Send + 'static;
    type Metadata: Clone + Send + JobMetadata + 'static;

    const ROUND: AggregationRound;
    const SERVICE_NAME: &'static str;

    async fn process_job(
        job: Self::Job,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
        started_at: Instant,
    ) -> anyhow::Result<Self::OutputArtifacts>;

    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Keystore,
    ) -> anyhow::Result<Self::Job>;

    async fn get_metadata(
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> anyhow::Result<Option<Self::Metadata>>;
}

#[derive(Debug)]
pub struct WitnessGenerator<R> {
    pub config: FriWitnessGeneratorConfig,
    pub object_store: Arc<dyn ObjectStore>,
    pub connection_pool: ConnectionPool<Prover>,
    pub protocol_version: ProtocolSemanticVersion,
    pub keystore: Keystore,
    _round: PhantomData<R>,
}

impl<R> WitnessGenerator<R>
where
    R: JobManager + ArtifactsManager,
{
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
    ) -> Self {
        Self {
            config,
            object_store,
            connection_pool,
            protocol_version,
            keystore,
            _round: Default::default(),
        }
    }
}

#[async_trait]
impl<R> JobProcessor for WitnessGenerator<R>
where
    R: JobManager + ArtifactsManager + Send + Sync,
{
    type Job = R::Job;
    type JobId = JobId;
    type JobArtifacts = R::OutputArtifacts;

    const SERVICE_NAME: &'static str = R::SERVICE_NAME;

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        if let Some(job_metadata) =
            R::get_metadata(self.connection_pool.clone(), self.protocol_version)
                .await
                .context("get_metadata()")?
        {
            tracing::info!("Processing {:?} job {:?}", R::ROUND, job_metadata.job_id());
            Ok(Some((
                job_metadata.job_id(),
                R::prepare_job(job_metadata, &*self.object_store, self.keystore.clone())
                    .await
                    .context("prepare_job()")?,
            )))
        } else {
            Ok(None)
        }
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        self.connection_pool
            .connection()
            .await
            .expect("Failed to acquire DB connection for witness generator")
            .fri_witness_generator_dal()
            .mark_witness_job_failed(&error, job_id.id(), job_id.chain_id(), R::ROUND)
            .await
            .expect("Failed to mark witness job as failed");
    }

    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let object_store = self.object_store.clone();
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            R::process_job(job, object_store, max_circuits_in_flight, started_at).await
        })
    }

    #[tracing::instrument(skip_all, fields(job_id = %job_id))]
    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        tracing::info!("Saving {:?} artifacts for job {:?}", R::ROUND, job_id);

        let blob_save_started_at = Instant::now();

        let blob_urls = R::save_to_bucket(job_id, artifacts.clone(), &*self.object_store).await;

        WITNESS_GENERATOR_METRICS.blob_save_time[&R::ROUND.into()]
            .observe(blob_save_started_at.elapsed());

        tracing::info!("Saved {:?} artifacts for job {:?}", R::ROUND, job_id);
        R::save_to_database(
            &self.connection_pool,
            job_id,
            started_at,
            blob_urls,
            artifacts,
        )
        .await?;

        tracing::info!("Saved {:?} to database for job {:?}", R::ROUND, job_id);

        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &Self::JobId) -> anyhow::Result<u32> {
        let mut prover_storage = self.connection_pool.connection().await.context(format!(
            "failed to acquire DB connection for {:?}",
            R::ROUND
        ))?;
        prover_storage
            .fri_witness_generator_dal()
            .get_witness_job_attempts(job_id.id(), job_id.chain_id(), R::ROUND)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context(format!("failed to get job attempts for {:?}", R::ROUND))
    }
}
