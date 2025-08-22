//! Actual implementation of Web3 API namespaces logic, not tied to the backend
//! used to create a JSON RPC server.

mod debug;
mod en;
pub(crate) mod eth;
mod net;
mod snapshots;
mod unstable;
mod utils;
mod web3;
mod zks;

pub(crate) use utils::*;

pub(super) use self::{
    debug::DebugNamespace, en::EnNamespace, eth::EthNamespace, net::NetNamespace,
    snapshots::SnapshotsNamespace, unstable::UnstableNamespace, web3::Web3Namespace,
    zks::ZksNamespace,
};
