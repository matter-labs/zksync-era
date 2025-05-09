use std::time::Duration;

use zksync_eth_client::{CallFunctionArgs, ContractCallError, EthInterface};
use zksync_node_api_server::web3::state::BridgeAddressesHandle;
use zksync_types::{ethabi::Contract, Address, L2_ASSET_ROUTER_ADDRESS};
use zksync_web3_decl::{
    client::{DynClient, L2},
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
    pub l1_eth_client: Box<dyn EthInterface>,
    pub bridgehub_addr: Address,
    pub update_interval: Option<Duration>,
    pub bridgehub_abi: Contract,
    pub l1_asset_router_abi: Contract,
}

struct L1SharedBridgeInfo {
    l1_shared_bridge_addr: Address,
    should_use_l2_asset_router: bool,
}

impl L1UpdaterInner {
    async fn get_shared_bridge_info(&self) -> Result<L1SharedBridgeInfo, ContractCallError> {
        let l1_shared_bridge_addr: Address = CallFunctionArgs::new("sharedBridge", ())
            .for_contract(self.bridgehub_addr, &self.bridgehub_abi)
            .call(self.l1_eth_client.as_ref())
            .await?;

        let l1_nullifier_addr: Result<Address, ContractCallError> =
            CallFunctionArgs::new("L1_NULLIFIER", ())
                .for_contract(l1_shared_bridge_addr, &self.l1_asset_router_abi)
                .call(self.l1_eth_client.as_ref())
                .await;

        // In case we can successfully retrieve the l1 nullifier, this is definitely the new l1 asset router.
        // The contrary is not necessarily true: the query can fail either due to network issues or
        // due to the contract being outdated. To be conservative, we just always treat such cases as `false`.
        let should_use_l2_asset_router = l1_nullifier_addr.is_ok();

        Ok(L1SharedBridgeInfo {
            l1_shared_bridge_addr,
            should_use_l2_asset_router,
        })
    }

    async fn loop_iteration(&self) {
        match self.get_shared_bridge_info().await {
            Ok(info) => {
                self.bridge_address_updater
                    .update_l1_shared_bridge(info.l1_shared_bridge_addr)
                    .await;
                // We only update one way:
                // - Once the L2 asset router should be used, there is never a need to go back
                // - To not undo the previous change in case of a network error
                if info.should_use_l2_asset_router {
                    self.bridge_address_updater
                        .update_l2_bridges(L2_ASSET_ROUTER_ADDRESS)
                        .await;
                }
            }
            Err(err) => {
                tracing::error!("Failed to query shared bridge address, error: {err:?}");
            }
        }
    }
}

// Define the enum to hold either updater
#[allow(clippy::large_enum_variant)]
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
