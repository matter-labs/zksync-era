use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_keystore::keystore::Keystore;

#[async_trait]
pub trait WitnessGenerator {
    type Job: Send + 'static;
    type Metadata;
    type Artifacts;

    async fn process_job(
        job: Self::Job,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: Option<usize>,
        started_at: Instant,
    ) -> anyhow::Result<Self::Artifacts>;

    async fn prepare_job(
        metadata: Self::Metadata,
        object_store: &dyn ObjectStore,
        keystore: Keystore,
    ) -> anyhow::Result<Self::Job>;
}
