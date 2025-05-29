//! Structures exposed by the `IExecutor.sol`.

mod commit_batch_info;
mod stored_batch_info;
mod commit_boojum_os_batch_info;

pub const SUPPORTED_ENCODING_VERSION: u8 = 0;

#[cfg(test)]
mod tests;

pub use self::{
    commit_batch_info::{
        CommitBatchInfo, PUBDATA_SOURCE_BLOBS, PUBDATA_SOURCE_CALLDATA,
        PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY,
    },
    stored_batch_info::StoredBatchInfo,
    commit_boojum_os_batch_info::CommitBoojumOSBatchInfo,
};
