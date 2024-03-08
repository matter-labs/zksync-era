use std::sync::Arc;

use zksync_core::api_server::tx_sender::{master_pool_sink::MasterPoolSink, proxy::TxProxy};
use zksync_web3_decl::jsonrpsee::http_client::{transport::HttpBackend, HttpClient};

use crate::{
    implementations::resources::{pools::MasterPoolResource, web3_api::TxSinkResource},
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
#[non_exhaustive]
pub enum TxSinkLayer {
    MasterPoolSink,
    ProxySink { main_node_url: String },
}

#[async_trait::async_trait]
impl WiringLayer for TxSinkLayer {
    fn layer_name(&self) -> &'static str {
        "tx_sink_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let tx_sink = match self.as_ref() {
            TxSinkLayer::MasterPoolSink => {
                let pool = context
                    .get_resource::<MasterPoolResource>()
                    .await?
                    .get()
                    .await?;
                TxSinkResource(Arc::new(MasterPoolSink::new(pool)))
            }
            TxSinkLayer::ProxySink { main_node_url } => {
                let client = HttpClient::<HttpBackend>::builder()
                    .build(main_node_url)
                    .map_err(|err| WiringError::Internal(err.into()))?;
                TxSinkResource(Arc::new(TxProxy::new(client)))
            }
        };
        context.insert_resource(tx_sink)?;
        Ok(())
    }
}
