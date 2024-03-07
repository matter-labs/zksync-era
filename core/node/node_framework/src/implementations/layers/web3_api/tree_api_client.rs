use std::sync::Arc;

use zksync_core::api_server::tree::TreeApiHttpClient;

use crate::{
    implementations::resources::web3_api::TreeApiClientResource,
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
            let client = TreeApiHttpClient::new(url);
            context.insert_resource(TreeApiClientResource(Arc::new(client)))?;
        }
        Ok(())
    }
}
