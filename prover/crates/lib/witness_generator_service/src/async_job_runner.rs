use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_processor::{Backoff, BackoffAndCancellable, AsyncJobRunner};
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{artifact_manager::ArtifactsManager, executor::WitnessGeneratorExecutor, job_picker::WitnessGeneratorJobPicker, job_saver::WitnessGeneratorJobSaver, rounds::{JobManager, VerificationKeyManager}};

// #[derive(Debug)]
// pub struct WitnessGeneratorRunnerBuilder {
//     pool: ConnectionPool<Prover>,
//     blob_store: Arc<dyn ObjectStore>,
//     protocol_version: ProtocolSemanticVersion,
//     keystore: Arc<dyn VerificationKeyManager>,
//     max_circuits_in_flight: usize,
//     cancellation_token: CancellationToken,
// }

// impl WitnessGeneratorRunnerBuilder {
//     pub fn new(
//         pool: ConnectionPool<Prover>,
//         blob_store: Arc<dyn ObjectStore>,
//         protocol_version: ProtocolSemanticVersion,
//         keystore: Arc<dyn VerificationKeyManager>,
//         max_circuits_in_flight: usize,
//         cancellation_token: CancellationToken,
//     ) -> Self {
//         Self {
//             pool,
//             blob_store,
//             protocol_version,
//             keystore,
//             max_circuits_in_flight,
//             cancellation_token,
//         }
//     }

//     /// Witness Generator runner implementation.
//     pub fn witness_generator_runner<R>(
//         &self,
//     ) -> AsyncJobRunner<WitnessGeneratorExecutor<R>, WitnessGeneratorJobPicker<R>, WitnessGeneratorJobSaver<R>>
//     where
//     R: JobManager + ArtifactsManager,
//     {
//         let executor = WitnessGeneratorExecutor::new(
//             self.blob_store.clone(),
//             self.max_circuits_in_flight,
//         );
//         let job_picker = WitnessGeneratorJobPicker::new(
//             self.pool.clone(),
//             self.blob_store.clone(),
//             self.protocol_version,
//             self.keystore.clone(),
//         );
//         let job_saver = WitnessGeneratorJobSaver::new(
//             self.pool.clone(),
//             self.blob_store.clone(),
//         );
//         let backoff = Backoff::default();

//         AsyncJobRunner::new(
//             executor,
//             job_picker,
//             job_saver,
//             Some(BackoffAndCancellable::new(
//                 backoff,
//                 self.cancellation_token.clone(),
//             )),
//         )
//     }
// }

pub fn witness_generator_runner<R>(
    max_circuits_in_flight: usize,
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,

    protocol_version: ProtocolSemanticVersion,
    keystore: Arc<dyn VerificationKeyManager>,
    cancellation_token: CancellationToken,
) -> AsyncJobRunner<WitnessGeneratorExecutor<R>, WitnessGeneratorJobPicker<R>, WitnessGeneratorJobSaver<R>>
where
R: JobManager + ArtifactsManager,
{
    let executor = WitnessGeneratorExecutor::new(
        blob_store.clone(),
        max_circuits_in_flight,
    );
    let job_picker = WitnessGeneratorJobPicker::new(
        pool.clone(),
        blob_store.clone(),
        protocol_version,
        keystore.clone(),
    );
    let job_saver = WitnessGeneratorJobSaver::new(
        pool.clone(),
        blob_store.clone(),
    );
    let backoff = Backoff::default();

    AsyncJobRunner::new(
        executor,
        job_picker,
        job_saver,
        Some(BackoffAndCancellable::new(
            backoff,
            cancellation_token.clone(),
        )),
    )
}
