use std::{num::NonZero, path::PathBuf, str::FromStr, sync::Arc};

use zksync_basic_types::url::SensitiveUrl;
use zksync_utils::env::Workspace;
use zksync_web3_decl::client::{Client, DynClient, L1};

use crate::{
    l1_fetcher::fetcher::{
        L1Fetcher, L1FetcherConfig, ProtocolVersioning, ProtocolVersioning::AllVersions,
    },
    BlobClient, BlobHttpClient,
};

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

pub fn initial_states_directory() -> PathBuf {
    let base_path = Workspace::locate().core();
    dbg!(&base_path);
    base_path.join("node/l1_recovery/initial_states")
}

#[allow(unused)] // only used in tests
pub fn sepolia_initial_state_path() -> PathBuf {
    initial_states_directory().join("InitialStateSepolia.json")
}

#[allow(unused)] // only used in tests
pub fn mainnet_initial_state_path() -> PathBuf {
    initial_states_directory().join("InitialStateMainnet.json")
}

#[allow(unused)]
pub fn local_initial_state_path() -> PathBuf {
    initial_states_directory().join("InitialStateV25.json")
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
pub fn sepolia_l1_client() -> Box<DynClient<L1>> {
    let url = SensitiveUrl::from_str("https://ethereum-sepolia-rpc.publicnode.com").unwrap();
    let eth_client = Client::http(url)
        .unwrap()
        .with_allowed_requests_per_second(NonZero::new(10).unwrap())
        .build();
    Box::new(eth_client)
}

#[allow(unused)] // only used in tests
pub fn sepolia_l1_fetcher() -> L1Fetcher {
    let config = L1FetcherConfig {
        block_step: 10000,
        diamond_proxy_addr: sepolia_diamond_proxy_addr().parse().unwrap(),
        versioning: sepolia_versioning(),
    };
    L1Fetcher::new(config, sepolia_l1_client()).unwrap()
}

#[allow(unused)] // only used in tests
pub fn sepolia_blob_client() -> Arc<dyn BlobClient> {
    Arc::new(BlobHttpClient::new("https://api.sepolia.blobscan.com/blobs/").unwrap())
}
