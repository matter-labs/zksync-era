use std::sync::Arc;

use anyhow::Context;
use zksync_eth_client::clients::QueryClient;

use crate::{
    implementations::resources::eth_interface::EthInterfaceResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct QueryEthClientLayer {
    web3_url: String,
}

impl QueryEthClientLayer {
    pub fn new(web3_url: String) -> Self {
        Self { web3_url }
    }
}

#[async_trait::async_trait]
impl WiringLayer for QueryEthClientLayer {
    fn layer_name(&self) -> &'static str {
        "query_eth_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let query_client = QueryClient::new(&self.web3_url).context("QueryClient::new()")?;
        context.insert_resource(EthInterfaceResource(Arc::new(query_client)))?;
        Ok(())
    }
}
