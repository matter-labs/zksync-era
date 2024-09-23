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

use crate::artifacts::ArtifactsManager;

mod leaf_aggregation;
mod node_aggregation;
mod recursion_tip;
mod scheduler;

pub use leaf_aggregation::LeafAggregation;
pub use node_aggregation::NodeAggregation;
pub use recursion_tip::RecursionTip;
pub use scheduler::Scheduler;
use zksync_prover_fri_types::get_current_pod_name;
use zksync_prover_fri_utils::metrics::StageLabel;
use zksync_types::basic_fri_types::AggregationRound;

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::leaf_aggregation::{LeafAggregationArtifacts, LeafAggregationWitnessGeneratorJob},
};

#[async_trait]
pub trait JobManager {
    type Job: Send + 'static;
    type Metadata;
    type Artifacts;

    const ROUND: &'static str;

    async fn process_job(
        job: Self::Job,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
        started_at: Instant,
    ) -> anyhow::Result<Self::Artifacts>;

    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Keystore,
    ) -> anyhow::Result<Self::Job>;

    async fn get_job_attempts(
        connection_pool: ConnectionPool<Prover>,
        job_id: u32,
    ) -> anyhow::Result<u32>;

    async fn save_failure(
        connection_pool: ConnectionPool<Prover>,
        job_id: u32,
        error: String,
    ) -> anyhow::Result<()>;

    //async fn get_metadata(connection_pool: ConnectionPool<Prover>, protocol_version: ProtocolSemanticVersion) ->
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

impl<R> JobProcessor for WitnessGenerator<R>
where
    R: JobManager + ArtifactsManager,
{
    type Job = R::Job;
    type JobId = u32;
    type JobArtifacts = R::Artifacts;

    const SERVICE_NAME: &'static str = "";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        todo!()
    }

    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String) {
        todo!()
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

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        tracing::info!("Saving {:?} artifacts for job {:?}", R::ROUND, job_id);

        let blob_save_started_at = Instant::now();

        let blob_urls = Self::save_to_bucket(job_id, artifacts.clone(), &*self.object_store).await;

        WITNESS_GENERATOR_METRICS.blob_save_time[&StageLabel::from(R::ROUND)]
            .observe(blob_save_started_at.elapsed());

        tracing::info!("Saved {:?} artifacts for job {:?}", R::ROUND, job_id);
        Self::save_to_database(
            &self.connection_pool,
            job_id,
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

    async fn get_job_attempts(&self, job_id: &Self::JobId) -> anyhow::Result<u32> {
        R::get_job_attempts(self.connection_pool.clone(), *job_id).await
    }
}

#[async_trait]
impl JobProcessor for WitnessGenerator<LeafAggregation> {
    type Job = LeafAggregationWitnessGeneratorJob;
    type JobId = u32;
    type JobArtifacts = LeafAggregationArtifacts;

    const SERVICE_NAME: &'static str = "fri_leaf_aggregation_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some(metadata) = prover_connection
            .fri_witness_generator_dal()
            .get_next_leaf_aggregation_job(self.protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        tracing::info!("Processing leaf aggregation job {:?}", metadata.id);
        Ok(Some((
            metadata.id,
            <Self as JobManager>::prepare_job(metadata, &*self.object_store, self.keystore.clone())
                .await
                .context("prepare_leaf_aggregation_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) -> () {
        self.connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_leaf_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<LeafAggregationArtifacts>> {
        let object_store = self.object_store.clone();
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            <Self as JobManager>::process_job(
                job,
                object_store,
                Some(max_circuits_in_flight),
                started_at,
            )
            .await
        })
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: LeafAggregationArtifacts,
    ) -> anyhow::Result<()> {
        let block_number = artifacts.block_number;
        let circuit_id = artifacts.circuit_id;
        tracing::info!(
            "Saving leaf aggregation artifacts for block {} with circuit {}",
            block_number.0,
            circuit_id,
        );

        let blob_save_started_at = Instant::now();

        let blob_urls = Self::save_to_bucket(job_id, artifacts.clone(), &*self.object_store).await;

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::LeafAggregation.into()]
            .observe(blob_save_started_at.elapsed());

        tracing::info!(
            "Saved leaf aggregation artifacts for block {} with circuit {}",
            block_number.0,
            circuit_id,
        );
        Self::save_to_database(
            &self.connection_pool,
            job_id,
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

    async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for LeafAggregationWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_leaf_aggregation_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for LeafAggregationWitnessGenerator")
    }
}
