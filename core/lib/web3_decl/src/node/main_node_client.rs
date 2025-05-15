use std::{num::NonZeroUsize, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use zksync_health_check::{AppHealthCheck, CheckHealth, Health, HealthStatus};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::{url::SensitiveUrl, L2ChainId};

use super::resources::MainNodeClientResource;
use crate::{
    client::{Client, DynClient, L2},
    namespaces::EthNamespaceClient,
};

/// Wiring layer for main node client.
#[derive(Debug)]
pub struct MainNodeClientLayer {
    url: SensitiveUrl,
    rate_limit_rps: NonZeroUsize,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    main_node_client: MainNodeClientResource,
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
            .insert_custom_component(Arc::new(MainNodeHealthCheck::from(client.clone())))
            .map_err(WiringError::internal)?;

        Ok(Output {
            main_node_client: client.into(),
        })
    }
}

/// Main node health check.
#[derive(Debug)]
struct MainNodeHealthCheck(Box<DynClient<L2>>);

impl From<Box<DynClient<L2>>> for MainNodeHealthCheck {
    fn from(client: Box<DynClient<L2>>) -> Self {
        Self(client.for_component("main_node_health_check"))
    }
}

#[async_trait]
impl CheckHealth for MainNodeHealthCheck {
    fn name(&self) -> &'static str {
        "main_node_http_rpc"
    }

    async fn check_health(&self) -> Health {
        if let Err(err) = self.0.get_block_number().await {
            tracing::warn!("Health-check call to main node HTTP RPC failed: {err}");
            let details = serde_json::json!({
                "error": err.to_string(),
            });
            return Health::from(HealthStatus::NotReady).with_details(details);
        }
        HealthStatus::Ready.into()
    }
}
