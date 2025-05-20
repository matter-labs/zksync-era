use std::time::Duration;

/// Configuration for the Gateway Migrator crate.
#[derive(Debug, Clone, PartialEq)]
pub struct GatewayMigratorConfig {
    /// How often we want to poll the Ethereum node.
    pub eth_node_poll_interval: Duration,
}

impl Default for GatewayMigratorConfig {
    fn default() -> Self {
        Self {
            eth_node_poll_interval: Duration::from_secs(12),
        }
    }
}
