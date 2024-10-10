//! Structures exposed by the `IExecutor.sol`.

mod commit_batch_info;
mod stored_batch_info;

#[cfg(test)]
mod tests;

pub use self::{commit_batch_info::CommitBatchInfo, stored_batch_info::StoredBatchInfo};
