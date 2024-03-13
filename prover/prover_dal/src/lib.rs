use std::time::Duration;

use sqlx::{postgres::types::PgInterval, types::chrono::NaiveTime, PgConnection};
use zksync_db_connection::processor::{
    async_trait, BasicStorageProcessor, StorageKind, StorageProcessorTags,
};
pub use zksync_db_connection::{connection::ConnectionPool, processor::StorageProcessor};

use crate::{
    fri_gpu_prover_queue_dal::FriGpuProverQueueDal,
    fri_proof_compressor_dal::FriProofCompressorDal,
    fri_protocol_versions_dal::FriProtocolVersionsDal, fri_prover_dal::FriProverDal,
    fri_scheduler_dependency_tracker_dal::FriSchedulerDependencyTrackerDal,
    fri_witness_generator_dal::FriWitnessGeneratorDal,
};

pub mod fri_gpu_prover_queue_dal;
pub mod fri_proof_compressor_dal;
pub mod fri_protocol_versions_dal;
pub mod fri_prover_dal;
pub mod fri_scheduler_dependency_tracker_dal;
pub mod fri_witness_generator_dal;

#[derive(Clone)]
pub struct Prover(());

#[derive(Debug)]
pub struct ProverProcessor<'a>(BasicStorageProcessor<'a>);

impl<'a> From<BasicStorageProcessor<'a>> for ProverProcessor<'a> {
    fn from(storage: BasicStorageProcessor<'a>) -> Self {
        Self(storage)
    }
}

impl StorageKind for Prover {
    type Processor<'a> = ProverProcessor<'a>;
}

#[async_trait]
impl StorageProcessor for ProverProcessor<'_> {
    type Processor<'a> = ProverProcessor<'a> where Self: 'a;

    async fn start_transaction(&mut self) -> sqlx::Result<ProverProcessor<'_>> {
        self.0.start_transaction().await.map(ProverProcessor::from)
    }

    /// Checks if the `StorageProcessor` is currently within database transaction.
    fn in_transaction(&self) -> bool {
        self.0.in_transaction()
    }

    async fn commit(self) -> sqlx::Result<()> {
        self.0.commit().await
    }

    fn conn(&mut self) -> &mut PgConnection {
        self.0.conn_and_tags().0
    }

    fn conn_and_tags(&mut self) -> (&mut PgConnection, Option<&StorageProcessorTags>) {
        self.0.conn_and_tags()
    }
}

impl<'a> ProverProcessor<'a> {
    pub fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<'_, 'a> {
        FriWitnessGeneratorDal { storage: self }
    }

    pub fn fri_prover_jobs_dal(&mut self) -> FriProverDal<'_, 'a> {
        FriProverDal { storage: self }
    }

    pub fn fri_scheduler_dependency_tracker_dal(
        &mut self,
    ) -> FriSchedulerDependencyTrackerDal<'_, 'a> {
        FriSchedulerDependencyTrackerDal { storage: self }
    }

    pub fn fri_gpu_prover_queue_dal(&mut self) -> FriGpuProverQueueDal<'_, 'a> {
        FriGpuProverQueueDal { storage: self }
    }

    pub fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<'_, 'a> {
        FriProtocolVersionsDal { storage: self }
    }

    pub fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<'_, 'a> {
        FriProofCompressorDal { storage: self }
    }
}
pub fn duration_to_naive_time(duration: Duration) -> NaiveTime {
    let total_seconds = duration.as_secs() as u32;
    NaiveTime::from_hms_opt(
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60,
    )
    .unwrap()
}

pub const fn pg_interval_from_duration(processing_timeout: Duration) -> PgInterval {
    PgInterval {
        months: 0,
        days: 0,
        microseconds: processing_timeout.as_micros() as i64,
    }
}
