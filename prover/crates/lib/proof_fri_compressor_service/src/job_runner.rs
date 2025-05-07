use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_processor::{Backoff, BackoffAndCancellable, JobRunner};
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::{ProofFriCompressorExecutor, ProofFriCompressorJobPicker, ProofFriCompressorJobSaver};

#[derive(Debug)]
pub struct ProofFriCompressorRunnerBuilder {
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
    is_fflonk: bool,
    cancellation_token: CancellationToken,
}

impl ProofFriCompressorRunnerBuilder {
    pub fn new(
        pool: ConnectionPool<Prover>,
        blob_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
        is_fflonk: bool,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            pool,
            blob_store,
            protocol_version,
            keystore,
            is_fflonk,
            cancellation_token,
        }
    }

    /// Proof Fri Compressor runner implementation.
    pub fn proof_fri_compressor_runner(
        &self,
    ) -> JobRunner<
        ProofFriCompressorExecutor,
        ProofFriCompressorJobPicker,
        ProofFriCompressorJobSaver,
    > {
        let executor = ProofFriCompressorExecutor;
        let job_picker = ProofFriCompressorJobPicker::new(
            self.pool.clone(),
            self.blob_store.clone(),
            self.protocol_version,
            self.keystore.clone(),
            self.is_fflonk,
        );
        let job_saver = ProofFriCompressorJobSaver::new(
            self.pool.clone(),
            self.blob_store.clone(),
            self.protocol_version,
        );
        let backoff = Backoff::default();

        JobRunner::new(
            executor,
            job_picker,
            job_saver,
            1,
            Some(BackoffAndCancellable::new(
                backoff,
                self.cancellation_token.clone(),
            )),
        )
    }
}
