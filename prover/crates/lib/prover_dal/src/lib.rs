use zksync_db_connection::connection::DbMarker;
pub use zksync_db_connection::{
    connection::Connection,
    connection_pool::ConnectionPool,
    error::DalError,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::{
    cli_test_dal::CliTestDal,
    fri_proof_compressor_dal::FriProofCompressorDal,
    fri_protocol_versions_dal::FriProtocolVersionsDal,
    fri_prover_dal::FriProverDal,
    fri_witness_generator_dal::{
        basic::FriBasicWitnessGeneratorDal, leaf::FriLeafWitnessGeneratorDal,
        node::FriNodeWitnessGeneratorDal, recursion_tip::FriRecursionTipWitnessGeneratorDal,
        scheduler::FriSchedulerWitnessGeneratorDal, FriWitnessGeneratorDal,
    },
};

pub mod cli_test_dal;
pub mod fri_proof_compressor_dal;
pub mod fri_protocol_versions_dal;
pub mod fri_prover_dal;
pub mod fri_witness_generator_dal;

// This module is private and serves as a way to seal the trait.
mod private {
    pub trait Sealed {}
}

// Here we are making the trait sealed, because it should be public to function correctly, but we don't
// want to allow any other downstream implementations of this trait.
pub trait ProverDal<'a>: private::Sealed
where
    Self: 'a,
{
    fn cli_test_dal(&mut self) -> CliTestDal<'_, 'a>;

    fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<'_, 'a>;

    fn fri_basic_witness_generator_dal(&mut self) -> FriBasicWitnessGeneratorDal<'_, 'a>;

    fn fri_leaf_witness_generator_dal(&mut self) -> FriLeafWitnessGeneratorDal<'_, 'a>;

    fn fri_node_witness_generator_dal(&mut self) -> FriNodeWitnessGeneratorDal<'_, 'a>;

    fn fri_recursion_tip_witness_generator_dal(
        &mut self,
    ) -> FriRecursionTipWitnessGeneratorDal<'_, 'a>;

    fn fri_scheduler_witness_generator_dal(&mut self) -> FriSchedulerWitnessGeneratorDal<'_, 'a>;

    fn fri_prover_jobs_dal(&mut self) -> FriProverDal<'_, 'a>;

    fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<'_, 'a>;

    fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<'_, 'a>;
}

#[derive(Clone, Debug)]
pub struct Prover;

// Implement the marker trait for the Prover to be able to use it in Connection.
impl DbMarker for Prover {}
// Implement the sealed trait for the Connection.
impl private::Sealed for Connection<'_, Prover> {}

impl<'a> ProverDal<'a> for Connection<'a, Prover> {
    fn cli_test_dal(&mut self) -> CliTestDal<'_, 'a> {
        CliTestDal { storage: self }
    }

    fn fri_witness_generator_dal(&mut self) -> FriWitnessGeneratorDal<'_, 'a> {
        FriWitnessGeneratorDal { storage: self }
    }

    fn fri_basic_witness_generator_dal(&mut self) -> FriBasicWitnessGeneratorDal<'_, 'a> {
        FriBasicWitnessGeneratorDal { storage: self }
    }

    fn fri_leaf_witness_generator_dal(&mut self) -> FriLeafWitnessGeneratorDal<'_, 'a> {
        FriLeafWitnessGeneratorDal { storage: self }
    }

    fn fri_node_witness_generator_dal(&mut self) -> FriNodeWitnessGeneratorDal<'_, 'a> {
        FriNodeWitnessGeneratorDal { storage: self }
    }
    fn fri_recursion_tip_witness_generator_dal(
        &mut self,
    ) -> FriRecursionTipWitnessGeneratorDal<'_, 'a> {
        FriRecursionTipWitnessGeneratorDal { storage: self }
    }

    fn fri_scheduler_witness_generator_dal(&mut self) -> FriSchedulerWitnessGeneratorDal<'_, 'a> {
        FriSchedulerWitnessGeneratorDal { storage: self }
    }

    fn fri_prover_jobs_dal(&mut self) -> FriProverDal<'_, 'a> {
        FriProverDal { storage: self }
    }

    fn fri_protocol_versions_dal(&mut self) -> FriProtocolVersionsDal<'_, 'a> {
        FriProtocolVersionsDal { storage: self }
    }

    fn fri_proof_compressor_dal(&mut self) -> FriProofCompressorDal<'_, 'a> {
        FriProofCompressorDal { storage: self }
    }
}
