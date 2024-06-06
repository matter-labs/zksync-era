//! Structures exposed by the `IExecutor.sol`.

mod commit_batch_info;
mod stored_batch_info;

pub use self::{commit_batch_info::CommitBatchInfo, stored_batch_info::StoredBatchInfo};
