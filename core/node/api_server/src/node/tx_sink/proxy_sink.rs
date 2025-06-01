use std::sync::Arc;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_web3_decl::client::{DynClient, L2};

use crate::tx_sender::{
    proxy::{AccountNonceSweeperTask, TxProxy},
    tx_sink::TxSink,
};

/// Wiring layer for [`TxProxy`], [`TxSink`] implementation.
#[derive(Debug)]
pub struct ProxySinkLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    main_node_client: Box<DynClient<L2>>,
    master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    tx_sink: Arc<dyn TxSink>,
    #[context(task)]
    account_nonce_sweeper_task: AccountNonceSweeperTask,
}

#[async_trait::async_trait]
impl WiringLayer for ProxySinkLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "proxy_sink_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let proxy = TxProxy::new(input.main_node_client);
        let pool = input.master_pool.get_singleton().await?;
        let task = proxy.account_nonce_sweeper_task(pool);

        Ok(Output {
            tx_sink: Arc::new(proxy),
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
