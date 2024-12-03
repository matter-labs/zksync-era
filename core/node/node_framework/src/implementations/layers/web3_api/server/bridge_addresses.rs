use std::time::Duration;

use zksync_node_api_server::web3::state::BridgeAddressesHandle;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::ZksNamespaceClient,
};

use crate::{StopReceiver, Task, TaskId};

#[derive(Debug)]
pub struct BridgeAddressesUpdaterTask {
    pub bridge_address_updater: BridgeAddressesHandle,
    pub main_node_client: Box<DynClient<L2>>,
    pub update_interval: Option<Duration>,
}

#[async_trait::async_trait]
impl Task for BridgeAddressesUpdaterTask {
    fn id(&self) -> TaskId {
        "bridge_addresses_updater_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

        let update_interval = self.update_interval.unwrap_or(DEFAULT_INTERVAL);
        while !*stop_receiver.0.borrow_and_update() {
            match self.main_node_client.get_bridge_contracts().await {
                Ok(bridge_addresses) => {
                    self.bridge_address_updater.update(bridge_addresses).await;
                }
                Err(err) => {
                    tracing::error!("Failed to query `get_bridge_contracts`, error: {err:?}");
                }
            }

            if tokio::time::timeout(update_interval, stop_receiver.0.changed())
                .await
                .is_ok()
            {
                break;
            }
        }

        Ok(())
    }
}
