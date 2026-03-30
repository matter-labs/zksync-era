use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    jsonrpsee,
    namespaces::EnNamespaceClient as _,
};

#[derive(Debug)]
pub(super) struct MainNodeInteropFeeUpdateTask {
    interop_fee: Arc<AtomicU64>,
    main_node_client: Box<DynClient<L2>>,
    poll_interval: Duration,
}

impl MainNodeInteropFeeUpdateTask {
    pub(super) fn new(
        interop_fee: Arc<AtomicU64>,
        main_node_client: Box<DynClient<L2>>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            interop_fee,
            main_node_client,
            poll_interval,
        }
    }
}

#[async_trait::async_trait]
impl Task for MainNodeInteropFeeUpdateTask {
    fn id(&self) -> TaskId {
        "main_node_interop_fee_update_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        while !*stop_receiver.0.borrow_and_update() {
            match self.main_node_client.get_interop_fee().await {
                Ok(interop_fee) => {
                    self.interop_fee.store(interop_fee, Ordering::Relaxed);
                }
                Err(jsonrpsee::core::client::Error::Call(error))
                    if error.code() == jsonrpsee::types::error::METHOD_NOT_FOUND_CODE =>
                {
                    // Method is not supported by the main node, do nothing.
                }
                Err(err) => {
                    tracing::error!("Failed to query `interopProtocolFee`, error: {err:#}");
                }
            }

            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(self.poll_interval, stop_receiver.0.changed())
                .await
                .ok();
        }
        Ok(())
    }
}
