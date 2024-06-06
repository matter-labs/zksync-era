use std::sync::Arc;

use zksync_node_api_server::tx_sender::{master_pool_sink::MasterPoolSink, proxy::TxProxy};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
#[non_exhaustive]
pub enum TxSinkLayer {
    MasterPoolSink,
    ProxySink,
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
                    .get_resource::<PoolResource<MasterPool>>()
                    .await?
                    .get()
                    .await?;
                TxSinkResource(Arc::new(MasterPoolSink::new(pool)))
            }
            TxSinkLayer::ProxySink => {
                let MainNodeClientResource(client) = context.get_resource().await?;
                TxSinkResource(Arc::new(TxProxy::new(client)))
            }
        };
        context.insert_resource(tx_sink)?;
        Ok(())
    }
}
