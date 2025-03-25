use std::time::Duration;

use zksync_config::configs::api::Web3JsonRpcConfig;
use zksync_node_api_server::tx_sender::{
    allow_list_service::AllowListService, master_pool_sink::MasterPoolSink,
    whitelisted_deploy_pool_sink::WhitelistedDeployPoolSink,
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
    pub rpc_config: Web3JsonRpcConfig,
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
        let master_pool_sink = MasterPoolSink::new(pool.clone());

        let tx_sink = if self.rpc_config.deployment_allowlist_sink {
            let allowlist_service = AllowListService::new(
                self.rpc_config.http_file_url.unwrap_or_default(),
                Duration::from_secs(self.rpc_config.refresh_interval_secs.unwrap_or_default()),
            );
            WhitelistedDeployPoolSink::new(master_pool_sink, allowlist_service).into()
        } else {
            master_pool_sink.into()
        };

        Ok(Output { tx_sink })
    }
}
