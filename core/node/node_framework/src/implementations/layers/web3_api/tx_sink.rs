use std::{collections::HashSet, sync::Arc};

use zksync_core::api_server::tx_sender::{
    deny_list_pool_sink::DenyListPoolSink, master_pool_sink::MasterPoolSink, proxy::TxProxy,
};
use zksync_types::Address;

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource, pools::MasterPoolResource,
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
    DenyListPoolSink { deny_list: HashSet<Address> },
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
            TxSinkLayer::ProxySink => {
                let MainNodeClientResource(client) = context.get_resource().await?;
                TxSinkResource(Arc::new(TxProxy::new(client)))
            }
            TxSinkLayer::DenyListPoolSink { deny_list } => {
                let pool = context
                    .get_resource::<MasterPoolResource>()
                    .await?
                    .get()
                    .await?;
                TxSinkResource(Arc::new(DenyListPoolSink::new(pool, deny_list.clone())))
            }
        };
        context.insert_resource(tx_sink)?;
        Ok(())
    }
}
