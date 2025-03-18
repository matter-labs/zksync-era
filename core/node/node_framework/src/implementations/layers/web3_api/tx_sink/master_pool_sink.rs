use zksync_config::configs::TxSinkConfig;
use zksync_node_api_server::tx_sender::{
    master_pool_sink::MasterPoolSink, whitelisted_deploy_pool_sink::WhitelistedDeployPoolSink,
};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`MasterPoolSink`], [`TxSink`](zksync_node_api_server::tx_sender::tx_sink::TxSink) implementation.
pub struct MasterPoolSinkLayer {
    pub tx_sink_config: TxSinkConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub tx_sink: TxSinkResource,
}

#[async_trait::async_trait]
impl WiringLayer for MasterPoolSinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "master_pook_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;

        let tx_sink = if self.tx_sink_config.deployment_allowlist_sink {
            WhitelistedDeployPoolSink::new(MasterPoolSink::new(pool.clone()), pool).into()
        } else {
            MasterPoolSink::new(pool).into()
        };

        Ok(Output { tx_sink })
    }
}
