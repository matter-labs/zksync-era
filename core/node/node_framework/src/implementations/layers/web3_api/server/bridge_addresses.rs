use std::time::Duration;

use zksync_eth_client::CallFunctionArgs;
use zksync_node_api_server::web3::state::BridgeAddressesHandle;
use zksync_types::{ethabi::Contract, Address};
use zksync_web3_decl::{
    client::{DynClient, L1, L2},
    namespaces::ZksNamespaceClient,
};

use crate::{StopReceiver, Task, TaskId};

const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct BridgeAddressesENUpdaterTask {
    pub bridge_address_updater: BridgeAddressesHandle,
    pub main_node_client: Box<DynClient<L2>>,
    pub update_interval: Option<Duration>,
}

impl BridgeAddressesENUpdaterTask {
    pub async fn loop_iteration(&self) {
        match self.main_node_client.get_bridge_contracts().await {
            Ok(bridge_addresses) => {
                self.bridge_address_updater.update(bridge_addresses).await;
            }
            Err(err) => {
                tracing::error!("Failed to query `get_bridge_contracts`, error: {err:?}");
            }
        }
    }
}

#[async_trait::async_trait]
impl Task for BridgeAddressesENUpdaterTask {
    fn id(&self) -> TaskId {
        "bridge_addresses_en_updater_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let update_interval = self.update_interval.unwrap_or(DEFAULT_INTERVAL);
        while !*stop_receiver.0.borrow_and_update() {
            self.loop_iteration().await;

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

#[derive(Debug)]
pub struct BridgeAddressesMainNodeUpdaterTask {
    pub bridge_address_updater: BridgeAddressesHandle,
    pub l1_client: Box<DynClient<L1>>,
    pub update_interval: Option<Duration>,
    pub bridgehub_address: Address,
    pub bridgehub_abi: Contract,
}

impl BridgeAddressesMainNodeUpdaterTask {
    pub async fn loop_iteration(&self) {
        let mut bridge_addresses = self.bridge_address_updater.read().await;
        let call_result = CallFunctionArgs::new("sharedBridge", ())
            .for_contract(self.bridgehub_address, &self.bridgehub_abi)
            .call(&self.l1_client)
            .await;
        match call_result {
            Ok(shared_bridge_address) => {
                bridge_addresses.l1_shared_default_bridge = Some(shared_bridge_address);
                self.bridge_address_updater.update(bridge_addresses).await;
            }
            Err(err) => {
                tracing::error!("Failed to query shared bridge address, error: {err:?}");
            }
        }
    }
}

#[async_trait::async_trait]
impl Task for BridgeAddressesMainNodeUpdaterTask {
    fn id(&self) -> TaskId {
        "bridge_addresses_main_node_updater_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let update_interval = self.update_interval.unwrap_or(DEFAULT_INTERVAL);
        while !*stop_receiver.0.borrow_and_update() {
            self.loop_iteration().await;

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
