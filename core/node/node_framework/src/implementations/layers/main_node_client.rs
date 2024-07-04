use std::{num::NonZeroUsize, sync::Arc};

use anyhow::Context;
use zksync_node_sync::MainNodeHealthCheck;
use zksync_types::{url::SensitiveUrl, L2ChainId};
use zksync_web3_decl::client::{Client, DynClient, L2};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource, main_node_client::MainNodeClientResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for main node client.
#[derive(Debug)]
pub struct MainNodeClientLayer {
    url: SensitiveUrl,
    rate_limit_rps: NonZeroUsize,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub main_node_client: MainNodeClientResource,
}

impl MainNodeClientLayer {
    pub fn new(url: SensitiveUrl, rate_limit_rps: NonZeroUsize, l2_chain_id: L2ChainId) -> Self {
        Self {
            url,
            rate_limit_rps,
            l2_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeClientLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "main_node_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_node_client = Client::http(self.url)
            .context("failed creating JSON-RPC client for main node")?
            .for_network(self.l2_chain_id.into())
            .with_allowed_requests_per_second(self.rate_limit_rps)
            .build();

        let client = Box::new(main_node_client) as Box<DynClient<L2>>;

        // Insert healthcheck
        input
            .app_health
            .0
            .insert_custom_component(Arc::new(MainNodeHealthCheck::from(client.clone())))
            .map_err(WiringError::internal)?;

        Ok(Output {
            main_node_client: client.into(),
        })
    }
}
