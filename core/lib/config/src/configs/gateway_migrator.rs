use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

/// Configuration for the Gateway Migrator crate.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct GatewayMigratorConfig {
    /// How often we want to poll the Ethereum node.
    #[config(default_t = Duration::from_secs(12))]
    pub eth_node_poll_interval: Duration,
}
