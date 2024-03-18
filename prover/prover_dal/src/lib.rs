pub use zksync_db_connection::{
    connection::ConnectionPool,
    processor::StorageProcessor,
    utils::{duration_to_naive_time, pg_interval_from_duration},
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

#[derive(Clone, Debug)]
pub struct Prover;

pub trait ProverDals<'a>
where
    Self: 'a,
{
    fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<'_, 'a>;

    fn fri_prover_jobs_dal(&mut self) -> FriProverDal<'_, 'a>;

    fn fri_scheduler_dependency_tracker_dal(&mut self) -> FriSchedulerDependencyTrackerDal<'_, 'a>;

    fn fri_gpu_prover_queue_dal(&mut self) -> FriGpuProverQueueDal<'_, 'a>;

    fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<'_, 'a>;

    fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<'_, 'a>;
}

impl<'a> ProverDals<'a> for StorageProcessor<'a, Prover> {
    fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<'_, 'a> {
        FriWitnessGeneratorDal { storage: self }
    }

    fn fri_prover_jobs_dal(&mut self) -> FriProverDal<'_, 'a> {
        FriProverDal { storage: self }
    }

    fn fri_scheduler_dependency_tracker_dal(&mut self) -> FriSchedulerDependencyTrackerDal<'_, 'a> {
        FriSchedulerDependencyTrackerDal { storage: self }
    }

    fn fri_gpu_prover_queue_dal(&mut self) -> FriGpuProverQueueDal<'_, 'a> {
        FriGpuProverQueueDal { storage: self }
    }

    fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<'_, 'a> {
        FriProtocolVersionsDal { storage: self }
    }

    fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<'_, 'a> {
        FriProofCompressorDal { storage: self }
    }
}
