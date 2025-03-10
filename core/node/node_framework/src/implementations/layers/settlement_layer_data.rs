use anyhow::Context;
use zksync_config::configs::contracts::{chain::L2Contracts, ecosystem::L1SpecificContracts};
use zksync_contracts::getters_facet_contract;
use zksync_contracts_loader::{get_settlement_layer, load_sl_contracts};
use zksync_eth_client::EthInterface;
use zksync_types::{settlement::SettlementMode, Address, L2ChainId, L2_BRIDGEHUB_ADDRESS};
use zksync_web3_decl::namespaces::ZksNamespaceClient;

use crate::{
    implementations::resources::{
        contracts::{
            L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
            SettlementLayerContractsResource,
        },
        eth_interface::{EthInterfaceResource, L2InterfaceResource},
        settlement_layer::{SettlementModeResource, SlChainIdResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`SettlementLayerData`].
#[derive(Debug)]
pub struct SettlementLayerData {
    l2_contracts: L2Contracts,
    l1_specific_contracts: L1SpecificContracts,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
}

impl SettlementLayerData {
    pub fn new(
        l1_specific_contracts: L1SpecificContracts,
        l2_contracts: L2Contracts,
        l2_chain_id: L2ChainId,
        multicall3: Option<Address>,
    ) -> Self {
        Self {
            l2_contracts,
            l1_specific_contracts,
            l2_chain_id,
            multicall3,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub l2_eth_client: Option<L2InterfaceResource>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    initial_settlement_mode: SettlementModeResource,
    contracts: SettlementLayerContractsResource,
    l1_ecosystem_contracts: L1EcosystemContractsResource,
    l1_contracts: L1ChainContractsResource,
    sl_chain_id: SlChainIdResource,
    l2_contracts: L2ContractsResource,
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerData {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "settlement_layer_data"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let sl_l1_contracts = load_sl_contracts(
            &input.eth_client.0,
            self.l1_specific_contracts.bridge_hub.unwrap(),
            self.l2_chain_id,
            self.multicall3,
        )
        .await?
        .context("No diamond proxy deployed for chain id on L1")?;
        let initial_sl_mode = get_settlement_layer(
            &input.eth_client.0,
            sl_l1_contracts.chain_contracts_config.diamond_proxy_addr,
            &getters_facet_contract(),
        )
        .await?;

        let (sl_chain_id, sl_chain_contracts) = match initial_sl_mode {
            SettlementMode::SettlesToL1 => {
                let chain_id = input
                    .eth_client
                    .0
                    .as_ref()
                    .fetch_chain_id()
                    .await
                    .context("Failed to fetch chain id")?;
                (chain_id, sl_l1_contracts.clone())
            }
            SettlementMode::Gateway => {
                let client = input
                    .l2_eth_client
                    .expect("Should be present for gateway settlement mode")
                    .0;
                let chain_id = client
                    .fetch_chain_id()
                    .await
                    .context("Failed to fetch chain id")?;
                let l2_multicall3 = client
                    .get_l2_multicall3()
                    .await
                    .context("Failed to fecth multicall3")?;
                let sl_contracts = load_sl_contracts(
                    &client,
                    L2_BRIDGEHUB_ADDRESS,
                    self.l2_chain_id,
                    l2_multicall3,
                )
                .await?
                .context("No diamond proxy deployed for chain id on Gateway")?;
                (chain_id, sl_contracts)
            }
        };

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(initial_sl_mode),
            contracts: SettlementLayerContractsResource(sl_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(
                self.l1_specific_contracts.clone(),
            ),
            l2_contracts: L2ContractsResource(self.l2_contracts),
            l1_contracts: L1ChainContractsResource(sl_l1_contracts),
            sl_chain_id: SlChainIdResource(sl_chain_id),
        })
    }
}
