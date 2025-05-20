use std::time::Duration;

use zksync_config::configs;
use zksync_protobuf::ProtoRepr;

impl ProtoRepr for crate::proto::config::gateway_migrator::GatewayMigratorConfig {
    type Type = configs::GatewayMigratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            eth_node_poll_interval: self
                .eth_node_poll_interval_ms
                .map(Duration::from_millis)
                .unwrap_or(Duration::from_secs(12)),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            eth_node_poll_interval_ms: Some(this.eth_node_poll_interval.as_millis() as u64),
        }
    }
}
