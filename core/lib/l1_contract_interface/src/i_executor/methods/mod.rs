//! Utilities for encoding input data for methods defined in `IExecutor.sol`.

pub use self::{
    commit_batches::CommitBatches, execute_batches::ExecuteBatches,
    precommit_batches::PrecommitBatches, prove_batches::ProveBatches,
};

mod commit_batches;
mod execute_batches;
mod precommit_batches;
mod prove_batches;
