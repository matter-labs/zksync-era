use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_processor::{Backoff, BackoffAndCancellable, JobRunner};
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{ProofFriCompressorExecutor, ProofFriCompressorJobPicker, ProofFriCompressorJobSaver};

/// Proof Fri Compressor runner implementation.
pub fn proof_fri_compressor_runner(
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
    is_fflonk: bool,
    cancellation_token: CancellationToken,
) -> JobRunner<ProofFriCompressorExecutor, ProofFriCompressorJobPicker, ProofFriCompressorJobSaver>
{
    let executor = ProofFriCompressorExecutor::new(is_fflonk, keystore.clone(), protocol_version);
    let job_picker =
        ProofFriCompressorJobPicker::new(pool.clone(), blob_store.clone(), protocol_version);
    let job_saver =
        ProofFriCompressorJobSaver::new(pool.clone(), blob_store.clone(), protocol_version);
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
