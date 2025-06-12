use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_processor::JobPicker;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use super::executor::WitnessGeneratorExecutor;
use crate::{artifact_manager::ArtifactsManager, rounds::{JobManager, JobMetadata, VerificationKeyManager}};

/// WitnessGenerator job picker implementation.
/// Picks job from database (via MetadataLoader) and gets data from object store.
#[derive(Debug)]
pub struct WitnessGeneratorJobPicker<R> {
    pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Arc<dyn VerificationKeyManager>,
    _marker: std::marker::PhantomData<R>,
}

impl<R> WitnessGeneratorJobPicker<R> {
    pub fn new(
        pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Arc<dyn VerificationKeyManager>,
    ) -> Self {
        Self {
            pool,
            object_store,
            protocol_version,
            keystore,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<R> JobPicker for WitnessGeneratorJobPicker<R> 
where
    R: JobManager + ArtifactsManager,
{
    type ExecutorType = WitnessGeneratorExecutor<R>;

    async fn pick_job(
        &mut self,
    ) -> anyhow::Result<Option<(R::Job, R::Metadata)>> {
        tracing::info!("Started picking witness generator {:?} job", R::ROUND);

        if let Some(job_metadata) =
            R::get_metadata(self.pool.clone(), self.protocol_version)
                .await
                .context("get_metadata()")?
        {
            tracing::info!("Processing {:?} job {:?}", R::ROUND, job_metadata.job_id());
            let job = R::prepare_job(job_metadata.clone(), &*self.object_store, self.keystore.clone())
                .await
                .context("prepare_job()")?;
            Ok(Some((
                job,
                job_metadata,
            )))
        } else {
            Ok(None)
        }
    }
}
