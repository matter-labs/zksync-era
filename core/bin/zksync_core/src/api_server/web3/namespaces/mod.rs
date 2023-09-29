//! Actual implementation of Web3 API namespaces logic, not tied to the backend
//! used to create a JSON RPC server.

use std::time::Instant;

use zksync_types::api;

mod debug;
mod en;
mod eth;
mod eth_subscribe;
mod net;
mod web3;
mod zks;

pub use self::{
    debug::DebugNamespace,
    en::EnNamespace,
    eth::EthNamespace,
    eth_subscribe::{EthSubscribe, SubscriptionMap},
    net::NetNamespace,
    web3::Web3Namespace,
    zks::ZksNamespace,
};

/// Helper to report latency of an RPC method that takes `BlockId` as an argument. Used by `eth`
/// and `debug` namespaces.
fn report_latency_with_block_id_and_diff(
    method_name: &'static str,
    start: Instant,
    block_id: api::BlockId,
    block_diff: u32,
) {
    let block_diff_label = match block_diff {
        0 => "0",
        1 => "1",
        2 => "2",
        3..=9 => "<10",
        10..=99 => "<100",
        100..=999 => "<1000",
        _ => ">=1000",
    };
    metrics::histogram!(
        "api.web3.call",
        start.elapsed(),
        "method" => method_name,
        "block_id" => block_id.extract_block_tag(),
        "block_diff" => block_diff_label
    );

    metrics::histogram!("api.web3.call.block_diff", block_diff as f64, "method" => method_name);
}
