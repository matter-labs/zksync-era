use std::sync::Arc;

use zksync_metadata_calculator::api_server::TreeApiHttpClient;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource, web3_api::TreeApiClientResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer that provides the `TreeApiHttpClient` into the `ServiceContext` resources, if there is no
/// other client already inserted.
///
/// In case a client is already provided in the context, this layer does nothing.
///
/// ## Requests resources
///
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds resources
///
/// - `TreeApiClientResource` (if no such resource already exists)
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
            match context.insert_resource(TreeApiClientResource(client.clone())) {
                Ok(()) => {
                    // There was no client added before, we added one.
                }
                Err(WiringError::ResourceAlreadyProvided { .. }) => {
                    // Some other client was already added. We don't want to replace it.
                    return Ok(());
                }
                err @ Err(_) => {
                    // Propagate any other error.
                    return err;
                }
            }

            // Only provide the health check if necessary.
            let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
            app_health
                .insert_custom_component(client)
                .map_err(WiringError::internal)?;
        }
        Ok(())
    }
}
