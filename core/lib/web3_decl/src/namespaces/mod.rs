pub mod debug;
pub mod en;
pub mod eth;
pub mod eth_subscribe;
pub mod net;
pub mod snapshots;
pub mod web3;
pub mod zks;

// Server trait re-exports.
#[cfg(feature = "server")]
pub use self::{
    debug::DebugNamespaceServer, en::EnNamespaceServer, eth::EthNamespaceServer,
    net::NetNamespaceServer, snapshots::SnapshotsNamespaceServer, web3::Web3NamespaceServer,
    zks::ZksNamespaceServer,
};

// Client trait re-exports.
#[cfg(feature = "client")]
pub use self::{
    debug::DebugNamespaceClient, en::EnNamespaceClient, eth::EthNamespaceClient,
    net::NetNamespaceClient, snapshots::SnapshotsNamespaceClient, web3::Web3NamespaceClient,
    zks::ZksNamespaceClient,
};
