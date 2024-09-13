use std::time::Instant;
use zksync_object_store::ObjectStore;
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::JobProcessor;

pub trait WitnessGenerator {
    type Job: Send + 'static;
    type Metadata;
    type Artifacts;

    fn process_job(job: Self::Job, started_at: Instant) -> anyhow::Result<Self::Artifacts>;
    fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Option<Keystore>,
    ) -> Self::Job;
}
