//! Structures exposed by the `IExecutor.sol`.

mod commit_batch_info;
mod stored_batch_info;
pub const SUPPORTED_ENCODING_VERSION: u8 = 0;

pub use self::{commit_batch_info::CommitBatchInfo, stored_batch_info::StoredBatchInfo};
