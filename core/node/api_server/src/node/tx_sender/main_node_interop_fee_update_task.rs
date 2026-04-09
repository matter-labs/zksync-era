use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
};
use zksync_types::U256;
use zksync_web3_decl::{
    client::{DynClient, L2},
    jsonrpsee,
    namespaces::EnNamespaceClient as _,
};

use crate::tx_sender::InteropFeeProvider;

#[derive(Debug)]
pub(super) struct MainNodeInteropFeeProvider {
    interop_fee: RwLock<U256>,
    main_node_client: Box<DynClient<L2>>,
    poll_interval: Duration,
}

impl MainNodeInteropFeeProvider {
    pub(super) fn new(
        initial_interop_fee: U256,
        main_node_client: Box<DynClient<L2>>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            interop_fee: RwLock::new(initial_interop_fee),
            main_node_client,
            poll_interval,
        }
    }
}

#[async_trait::async_trait]
impl InteropFeeProvider for MainNodeInteropFeeProvider {
    async fn get_interop_fee(&self) -> U256 {
        *self
            .interop_fee
            .read()
            .expect("main node interop fee provider lock is poisoned")
    }
}

#[derive(Debug)]
pub(super) struct MainNodeInteropFeeUpdateTask {
    provider: Arc<MainNodeInteropFeeProvider>,
}

impl MainNodeInteropFeeUpdateTask {
    pub(super) fn new(provider: Arc<MainNodeInteropFeeProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait::async_trait]
impl Task for MainNodeInteropFeeUpdateTask {
    fn id(&self) -> TaskId {
        "main_node_interop_fee_update_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        while !*stop_receiver.0.borrow_and_update() {
            match self.provider.main_node_client.get_interop_fee().await {
                Ok(interop_fee) => {
                    *self
                        .provider
                        .interop_fee
                        .write()
                        .expect("main node interop fee provider lock is poisoned") = interop_fee;
                }
                Err(jsonrpsee::core::client::Error::Call(error))
                    if error.code() == jsonrpsee::types::error::METHOD_NOT_FOUND_CODE =>
                {
                    // Method is not supported by the main node, do nothing.
                }
                Err(err) => {
                    tracing::error!("Failed to query `interopProtocolFee`, error: {err:?}");
                }
            }

            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(self.provider.poll_interval, stop_receiver.0.changed())
                .await
                .ok();
        }
        Ok(())
    }
}
