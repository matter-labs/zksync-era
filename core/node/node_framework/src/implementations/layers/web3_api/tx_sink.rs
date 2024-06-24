use std::sync::Arc;

use zksync_node_api_server::tx_sender::{
    master_pool_sink::MasterPoolSink,
    proxy::{AccountNonceSweeperTask, TxProxy},
};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
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
                let proxy = TxProxy::new(client);

                let pool = context
                    .get_resource::<PoolResource<MasterPool>>()
                    .await?
                    .get_singleton()
                    .await?;
                let task = proxy.account_nonce_sweeper_task(pool);
                context.add_task(Box::new(task));

                TxSinkResource(Arc::new(proxy))
            }
        };
        context.insert_resource(tx_sink)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for AccountNonceSweeperTask {
    fn id(&self) -> TaskId {
        "account_nonce_sweeper_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
