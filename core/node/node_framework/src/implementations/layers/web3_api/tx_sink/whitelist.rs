use async_trait::async_trait;
use zksync_node_api_server::tx_sender::{
    master_pool_sink::MasterPoolSink, whitelist::WhitelistedDeployPoolSink,
};
use zksync_vm_executor::whitelist::{DeploymentTxFilter, SharedAllowList};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`WhitelistedDeployPoolSink`] that wraps a `MasterPoolSink` and enables allowlist filtering.
pub struct WhitelistedMasterPoolSinkLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub shared_allow_list: SharedAllowList,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub tx_sink: TxSinkResource,
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
        )
        .into();

        Ok(Output { tx_sink })
    }
}
