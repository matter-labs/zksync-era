use zksync_node_api_server::tx_sender::proxy::{AccountNonceSweeperTask, TxProxy};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        web3_api::TxSinkResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`TxProxy`], [`TxSink`](zksync_node_api_server::tx_sender::tx_sink::TxSink) implementation.
#[derive(Debug)]
pub struct TxProxySinkLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub main_node_client: MainNodeClientResource,
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub tx_sink: TxSinkResource,
    #[context(task)]
    pub account_nonce_sweeper_task: AccountNonceSweeperTask,
}

#[async_trait::async_trait]
impl WiringLayer for TxProxySinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tx_proxy_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let MainNodeClientResource(client) = input.main_node_client;
        let proxy = TxProxy::new(client);

        let pool = input.master_pool.get_singleton().await?;
        let task = proxy.account_nonce_sweeper_task(pool);

        Ok(Output {
            tx_sink: proxy.into(),
            account_nonce_sweeper_task: task,
        })
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
