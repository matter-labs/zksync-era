use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use zksync_contracts::{bridgehub_contract, l1_asset_router_contract};
use zksync_node_framework::{
    FromContext, IntoContext, StopReceiver, Task, TaskId, WiringError, WiringLayer,
};
use zksync_shared_resources::{api::BridgeAddressesHandle, contracts::L1ChainContractsResource};
use zksync_types::{ethabi::Contract, Address, L2_ASSET_ROUTER_ADDRESS};
use zksync_web3_decl::{
    client::{DynClient, L1, L2},
    namespaces::ZksNamespaceClient,
};

use crate::{CallFunctionArgs, ContractCallError, EthInterface};

#[derive(Debug)]
struct MainNodeUpdater {
    bridge_addresses: BridgeAddressesHandle,
    main_node_client: Box<DynClient<L2>>,
    update_interval: Duration,
}

impl MainNodeUpdater {
    async fn loop_iteration(&self) {
        match self.main_node_client.get_bridge_contracts().await {
            Ok(bridge_addresses) => {
                self.bridge_addresses.update(bridge_addresses).await;
            }
            Err(err) => {
                tracing::error!("Failed to query `get_bridge_contracts`, error: {:?}", err);
            }
        }
    }
}

#[derive(Debug)]
struct L1Updater {
    bridge_addresses: BridgeAddressesHandle,
    l1_eth_client: Box<dyn EthInterface>,
    bridgehub_addr: Address,
    update_interval: Duration,
    bridgehub_abi: Contract,
    l1_asset_router_abi: Contract,
}

struct L1SharedBridgeInfo {
    l1_shared_bridge_addr: Address,
    should_use_l2_asset_router: bool,
}

impl L1Updater {
    async fn get_shared_bridge_info(&self) -> Result<L1SharedBridgeInfo, ContractCallError> {
        let l1_shared_bridge_addr: Address = CallFunctionArgs::new("assetRouter", ())
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
                self.bridge_addresses
                    .update_l1_shared_bridge(info.l1_shared_bridge_addr)
                    .await;
                // We only update one way:
                // - Once the L2 asset router should be used, there is never a need to go back
                // - To not undo the previous change in case of a network error
                if info.should_use_l2_asset_router {
                    self.bridge_addresses
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
enum BridgeAddressesUpdaterTask {
    L1Updater(L1Updater),
    MainNodeUpdater(MainNodeUpdater),
}

impl BridgeAddressesUpdaterTask {
    async fn loop_iteration(&self) {
        match self {
            BridgeAddressesUpdaterTask::L1Updater(updater) => updater.loop_iteration().await,
            BridgeAddressesUpdaterTask::MainNodeUpdater(updater) => updater.loop_iteration().await,
        }
    }

    fn update_interval(&self) -> Duration {
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
        let update_interval = self.update_interval();
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

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    bridge_addresses: BridgeAddressesHandle,
    main_node_client: Option<Box<DynClient<L2>>>,
    l1_client: Box<DynClient<L1>>,
    l1_contracts: L1ChainContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    updater_task: BridgeAddressesUpdaterTask,
}

#[derive(Debug)]
pub struct BridgeAddressesUpdaterLayer {
    pub refresh_interval: Duration,
}

#[async_trait]
impl WiringLayer for BridgeAddressesUpdaterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "bridge_addresses_updater_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let updater_task = if let Some(main_node_client) = input.main_node_client {
            BridgeAddressesUpdaterTask::MainNodeUpdater(MainNodeUpdater {
                bridge_addresses: input.bridge_addresses,
                main_node_client,
                update_interval: self.refresh_interval,
            })
        } else {
            let l1_contracts = &input.l1_contracts.0.ecosystem_contracts;
            BridgeAddressesUpdaterTask::L1Updater(L1Updater {
                bridge_addresses: input.bridge_addresses,
                l1_eth_client: Box::new(input.l1_client),
                bridgehub_addr: l1_contracts
                    .bridgehub_proxy_addr
                    .context("Lacking l1 bridgehub proxy address")?,
                update_interval: self.refresh_interval,
                bridgehub_abi: bridgehub_contract(),
                l1_asset_router_abi: l1_asset_router_contract(),
            })
        };
        Ok(Output { updater_task })
    }
}
