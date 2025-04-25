use anyhow::Context;
use zksync_node_framework::{
    implementations::layers::interop_switch::InteropSwitchLayer,
    service::{ZkStackService, ZkStackServiceBuilder},
};
use zksync_types::url::SensitiveUrl;

pub struct InteropSwitchBuilder {
    node: ZkStackServiceBuilder,
    src_chains: Vec<SensitiveUrl>,
}

impl InteropSwitchBuilder {
    pub fn new(src_chains: Vec<SensitiveUrl>) -> anyhow::Result<Self> {
        Ok(Self {
            node: ZkStackServiceBuilder::new().context("Cannot create ZkStackServiceBuilder")?,
            src_chains,
        })
    }

    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.node.runtime_handle()
    }
    pub fn add_interop_switch_layer(mut self) -> anyhow::Result<Self> {
        self.node
            .add_layer(InteropSwitchLayer::new(self.src_chains.clone(), vec![]));
        Ok(self)
    }

    pub fn build(self) -> anyhow::Result<ZkStackService> {
        Ok(self.add_interop_switch_layer()?.node.build())
    }
}
