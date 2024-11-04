use std::path::PathBuf;

use zksync_utils::env::Workspace;

use crate::l1_fetcher::l1_fetcher::{ProtocolVersioning, ProtocolVersioning::AllVersions};

pub mod storage {
    /// The name of the index-to-key database folder.
    pub const INNER_DB_NAME: &str = "inner_db";
}

pub mod zksync {
    /// Bytes in raw L2 to L1 log.
    pub const L2_TO_L1_LOG_SERIALIZE_SIZE: usize = 88;
    /// The bitmask by applying which to the compressed state diff metadata we retrieve its operation.
    pub const OPERATION_BITMASK: u8 = 7;
    /// The number of bits shifting the compressed state diff metadata by which we retrieve its length.
    pub const LENGTH_BITS_OFFSET: u8 = 3;
    /// Size of `CommitBatchInfo.pubdataCommitments` item.
    pub const PUBDATA_COMMITMENT_SIZE: usize = 144;
    /// The number of trailing bytes to ignore when using calldata post-blobs. Contains unused blob commitments.
    pub const CALLDATA_SOURCE_TAIL_SIZE: usize = 32;
}

#[allow(unused)] // only used in tests
pub fn sepolia_initial_state_path() -> PathBuf {
    let base_path = Workspace::locate().core();
    base_path.join("core/node/l1_recovery/InitialStateSepolia.json")
}
#[allow(unused)] // only used in tests
pub fn mainnet_initial_state_path() -> PathBuf {
    let base_path = Workspace::locate().core();
    base_path.join("core/node/l1_recovery/InitialStateMainnet.json")
}

#[allow(unused)]
pub fn local_initial_state_path() -> PathBuf {
    let base_path = Workspace::locate().core();
    base_path.join("core/node/l1_recovery/InitialStateV24.json")
}

#[allow(unused)]
pub fn sepolia_versioning() -> ProtocolVersioning {
    AllVersions {
        v2_start_batch_number: 4_800_000,
        v3_start_batch_number: 5_435_346,
    }
}

#[allow(unused)]
pub fn mainnet_versioning() -> ProtocolVersioning {
    AllVersions {
        v2_start_batch_number: 18_715_403,
        v3_start_batch_number: 19_412_473,
    }
}

#[allow(unused)] // only used in tests
pub fn sepolia_diamond_proxy_addr() -> &'static str {
    "0x9a6de0f62Aa270A8bCB1e2610078650D539B1Ef9"
}

#[allow(unused)] // only used in tests
pub fn local_diamond_proxy_addr() -> &'static str {
    "0x426441939896362df986285b116051a9ed350baa"
}
