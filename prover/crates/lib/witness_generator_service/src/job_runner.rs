use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_processor::{Backoff, BackoffAndCancellable, JobRunner};
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{
    artifact_manager::ArtifactsManager,
    executor::WitnessGeneratorExecutor,
    job_picker::WitnessGeneratorJobPicker,
    job_saver::WitnessGeneratorJobSaver,
    rounds::{JobManager, VerificationKeyManager},
};

pub fn witness_generator_runner<R>(
    max_circuits_in_flight: usize,
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Arc<dyn VerificationKeyManager>,
    cancellation_token: CancellationToken,
) -> JobRunner<WitnessGeneratorExecutor<R>, WitnessGeneratorJobPicker<R>, WitnessGeneratorJobSaver<R>>
where
    R: JobManager + ArtifactsManager,
{
    let executor = WitnessGeneratorExecutor::new(blob_store.clone(), max_circuits_in_flight);
    let job_picker = WitnessGeneratorJobPicker::new(
        pool.clone(),
        blob_store.clone(),
        protocol_version,
        keystore.clone(),
    );
    let job_saver = WitnessGeneratorJobSaver::new(pool.clone(), blob_store.clone());
    let backoff = Backoff::default();

    JobRunner::new(
        executor,
        job_picker,
        job_saver,
        1,
        Some(BackoffAndCancellable::new(
            backoff,
            cancellation_token.clone(),
        )),
    )
}
