use std::sync::Arc;

use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::tree::TreeApiClient;

use crate::api_server::TreeApiHttpClient;

/// Wiring layer that provides the `TreeApiHttpClient` into the `ServiceContext` resources, if there is no
/// other client already inserted.
///
/// In case a client is already provided in the context, this layer does nothing.
#[derive(Debug)]
pub struct TreeApiClientLayer {
    url: Option<String>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    /// Fetched to check whether the `TreeApiClientResource` was already provided by another layer.
    tree_api_client: Option<Arc<dyn TreeApiClient>>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    tree_api_client: Option<Arc<dyn TreeApiClient>>,
}

impl TreeApiClientLayer {
    pub fn http(url: Option<String>) -> Self {
        Self { url }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TreeApiClientLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tree_api_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        if input.tree_api_client.is_some() {
            tracing::info!("Tree API client is already provided");
            return Ok(Output {
                tree_api_client: None,
            });
        }

        let Some(url) = &self.url else {
            tracing::info!("No Tree API client URL provided, not adding a fallback client");
            return Ok(Output {
                tree_api_client: None,
            });
        };

        let client = Arc::new(TreeApiHttpClient::new(url));
        input
            .app_health
            .insert_custom_component(client.clone())
            .map_err(WiringError::internal)?;
        Ok(Output {
            tree_api_client: Some(client),
        })
    }
}
