use std::sync::Arc;

use async_trait::async_trait;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_vm_executor::whitelist::{DeploymentTxFilter, SharedAllowList};

use crate::tx_sender::{
    master_pool_sink::MasterPoolSink, tx_sink::TxSink, whitelist::WhitelistedDeployPoolSink,
};

/// Wiring layer for [`WhitelistedDeployPoolSink`] that wraps a `MasterPoolSink` and enables allowlist filtering.
pub struct WhitelistedMasterPoolSinkLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    shared_allow_list: SharedAllowList,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    tx_sink: Arc<dyn TxSink>,
}

#[async_trait]
impl WiringLayer for WhitelistedMasterPoolSinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "whitelisted_master_pool_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let master_pool_sink = MasterPoolSink::new(pool);

        let tx_sink = WhitelistedDeployPoolSink::new(
            master_pool_sink,
            DeploymentTxFilter::new(input.shared_allow_list),
        );

        Ok(Output {
            tx_sink: Arc::new(tx_sink),
        })
    }
}
