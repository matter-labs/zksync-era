//! Consensus related metrics.

#[derive(Debug, vise::Metrics)]
#[metrics(prefix = "zksync_node_consensus")]
pub(crate) struct Metrics {
    /// Number of blocks that has been fetched via JSON-RPC.
    /// It is used only as a fallback when the p2p syncing is disabled or falling behind.
    /// so it shouldn't be increasing under normal circumstances if p2p syncing is enabled.
    pub fetch_block: vise::Counter,
}

#[vise::register]
pub(super) static METRICS: vise::Global<Metrics> = vise::Global::new();
