use std::sync::Arc;

use zksync_metadata_calculator::api_server::TreeApiHttpClient;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource, web3_api::TreeApiClientResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct TreeApiClientLayer {
    url: Option<String>,
}

impl TreeApiClientLayer {
    pub fn http(url: Option<String>) -> Self {
        Self { url }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TreeApiClientLayer {
    fn layer_name(&self) -> &'static str {
        "tree_api_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        if let Some(url) = &self.url {
            let client = Arc::new(TreeApiHttpClient::new(url));
            let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
            app_health
                .insert_custom_component(client.clone())
                .map_err(WiringError::internal)?;
            context.insert_resource(TreeApiClientResource(client))?;
        }
        Ok(())
    }
}
