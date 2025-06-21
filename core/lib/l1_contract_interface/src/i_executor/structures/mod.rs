//! Structures exposed by the `IExecutor.sol`.

use zksync_types::ProtocolVersionId;

mod commit_batch_info;
mod stored_batch_info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingVersion {
    PreInterop = 0,
    Supported = 1,
}

impl EncodingVersion {
    pub const fn latest() -> Self {
        Self::Supported
    }

    pub fn value(self) -> u8 {
        self as u8
    }
}

pub fn get_encoding_version(protocol_version: ProtocolVersionId) -> u8 {
    if protocol_version.is_pre_interop_fast_blocks() {
        EncodingVersion::PreInterop.value()
    } else {
        EncodingVersion::Supported.value()
    }
}

#[cfg(test)]
mod tests;

pub use self::{
    commit_batch_info::{
        CommitBatchInfo, PUBDATA_SOURCE_BLOBS, PUBDATA_SOURCE_CALLDATA,
        PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY,
    },
    stored_batch_info::StoredBatchInfo,
};
