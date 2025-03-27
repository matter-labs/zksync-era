use zksync_config::configs::api::DeploymentAllowlist;
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
    pub deployment_allowlist: Option<DeploymentAllowlist>,
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

        // Decide whether to create a WhitelistedDeployPoolSink or just use the MasterPoolSink.
        let tx_sink = match self.deployment_allowlist {
            Some(ref allowlist_cfg) => {
                if let Some(url) = allowlist_cfg.http_file_url() {
                    let allowlist_service =
                        AllowListService::new(url.to_string(), allowlist_cfg.refresh_interval());
                    WhitelistedDeployPoolSink::new(master_pool_sink, allowlist_service).into()
                } else {
                    master_pool_sink.into()
                }
            }
            None => master_pool_sink.into(),
        };

        Ok(Output { tx_sink })
    }
}
