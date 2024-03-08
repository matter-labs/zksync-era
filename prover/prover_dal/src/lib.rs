use sqlx::{pool::PoolConnection, PgConnection, Postgres};
use zksync_db_connection::processor::{
    StorageInteraction, StorageKind, StorageProcessor, StorageProcessorTags, TracedConnections,
};

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

pub struct Prover(());

pub struct ProverProcessor<'a>(StorageProcessor<'a>);

impl StorageKind for Prover {
    type Processor<'a> = ProverProcessor<'a>;
}

impl<'a> StorageInteraction for ProverProcessor<'a> {
    async fn start_transaction(&mut self) -> sqlx::Result<ProverProcessor<'_>> {
        self.0.start_transaction()
    }

    /// Checks if the `StorageProcessor` is currently within database transaction.
    fn in_transaction(&self) -> bool {
        self.0.in_transaction()
    }

    async fn commit(self) -> sqlx::Result<()> {
        self.0.commit()
    }

    fn from_pool(
        connection: PoolConnection<Postgres>,
        tags: Option<StorageProcessorTags>,
        traced_connections: Option<&TracedConnections>,
    ) -> Self {
        Self(StorageProcessor::from_pool(
            connection,
            tags,
            traced_connections,
        ))
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
