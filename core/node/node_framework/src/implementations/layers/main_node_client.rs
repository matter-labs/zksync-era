use std::{num::NonZeroUsize, sync::Arc};

use anyhow::Context;
use zksync_node_sync::MainNodeHealthCheck;
use zksync_types::{url::SensitiveUrl, L2ChainId};
use zksync_web3_decl::client::{Client, DynClient, L2};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource, main_node_client::MainNodeClientResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for main node client.
///
/// ## Requests resources
///
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds resources
///
/// - `MainNodeClientResource`
#[derive(Debug)]
pub struct MainNodeClientLayer {
    url: SensitiveUrl,
    rate_limit_rps: NonZeroUsize,
    l2_chain_id: L2ChainId,
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
    fn layer_name(&self) -> &'static str {
        "main_node_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let main_node_client = Client::http(self.url)
            .context("failed creating JSON-RPC client for main node")?
            .for_network(self.l2_chain_id.into())
            .with_allowed_requests_per_second(self.rate_limit_rps)
            .build();

        let client = Box::new(main_node_client) as Box<DynClient<L2>>;
        context.insert_resource(MainNodeClientResource(client.clone()))?;

        // Insert healthcheck
        let AppHealthCheckResource(app_health) = context.get_resource_or_default();
        app_health
            .insert_custom_component(Arc::new(MainNodeHealthCheck::from(client)))
            .map_err(WiringError::internal)?;

        Ok(())
    }
}
