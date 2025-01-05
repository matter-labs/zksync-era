use std::time::Duration;

use zksync_eth_client::{CallFunctionArgs, ContractCallError, EnrichedClientResult, EthInterface};
use zksync_node_api_server::web3::state::BridgeAddressesHandle;
use zksync_types::{
    ethabi::{decode, Contract, ParamType},
    web3, Address,
};
use zksync_web3_decl::{
    client::{DynClient, L1, L2},
    namespaces::ZksNamespaceClient,
};

use crate::{StopReceiver, Task, TaskId};

#[derive(Debug)]
pub struct MainNodeUpdaterInner {
    pub bridge_address_updater: BridgeAddressesHandle,
    pub main_node_client: Box<DynClient<L2>>,
    pub update_interval: Option<Duration>,
}

impl MainNodeUpdaterInner {
    async fn loop_iteration(&self) {
        match self.main_node_client.get_bridge_contracts().await {
            Ok(bridge_addresses) => {
                self.bridge_address_updater.update(bridge_addresses).await;
            }
            Err(err) => {
                tracing::error!("Failed to query `get_bridge_contracts`, error: {:?}", err);
            }
        }
    }
}

#[derive(Debug)]
pub struct L1UpdaterInner {
    pub bridge_address_updater: BridgeAddressesHandle,
    pub l1_eth_client: Box<DynClient<L1>>,
    pub bridgehub_addr: Address,
    pub update_interval: Option<Duration>,
    pub bridgehub_abi: Contract,
}

impl L1UpdaterInner {
    async fn loop_iteration(&self) {
        let call_result = CallFunctionArgs::new("sharedBridge", ())
            .for_contract(self.bridgehub_addr, &self.bridgehub_abi)
            .call(&self.l1_eth_client)
            .await;

        match call_result {
            Ok(shared_bridge_address) => {
                self.bridge_address_updater
                    .update_l1_shared_bridge(shared_bridge_address)
                    .await;
            }
            Err(err) => {
                tracing::error!("Failed to query shared bridge address, error: {err:?}");
            }
        }
    }
}

// Define the enum to hold either updater
#[derive(Debug)]
pub enum BridgeAddressesUpdaterTask {
    L1Updater(L1UpdaterInner),
    MainNodeUpdater(MainNodeUpdaterInner),
}

impl BridgeAddressesUpdaterTask {
    async fn loop_iteration(&self) {
        match self {
            BridgeAddressesUpdaterTask::L1Updater(updater) => updater.loop_iteration().await,
            BridgeAddressesUpdaterTask::MainNodeUpdater(updater) => updater.loop_iteration().await,
        }
    }

    fn update_interval(&self) -> Option<Duration> {
        match self {
            BridgeAddressesUpdaterTask::L1Updater(updater) => updater.update_interval,
            BridgeAddressesUpdaterTask::MainNodeUpdater(updater) => updater.update_interval,
        }
    }
}

#[async_trait::async_trait]
impl Task for BridgeAddressesUpdaterTask {
    fn id(&self) -> TaskId {
        "bridge_addresses_updater_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

        let update_interval = self.update_interval().unwrap_or(DEFAULT_INTERVAL);
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
