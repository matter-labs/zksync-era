use anyhow::Context;
use zksync_eth_client::clients::QueryClient;
use zksync_types::url::SensitiveUrl;

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct QueryEthClientLayer {
    web3_url: SensitiveUrl,
}

impl QueryEthClientLayer {
    pub fn new(web3_url: SensitiveUrl) -> Self {
        Self { web3_url }
    }
}

#[async_trait::async_trait]
impl WiringLayer for QueryEthClientLayer {
    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let query_client = QueryClient::new(self.web3_url.clone()).context("QueryClient::new()")?;
        context.insert_resource(EthInterfaceResource(Box::new(query_client)))?;
        Ok(())
    }
}
