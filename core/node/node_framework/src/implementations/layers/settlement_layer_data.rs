use anyhow::Context;
use zksync_config::{configs::contracts::ecosystem::L1SpecificContracts, SettlementLayerContracts};
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::EthInterface;
use zksync_gateway_migrator::get_settlement_layer;
use zksync_types::settlement::SettlementMode;

use crate::{
    implementations::resources::{
        contracts::{
            L1ChainContractsResource, L1EcosystemContractsResource,
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
    contracts: SettlementLayerContracts,
    l1_ecosystem_contracts: L1SpecificContracts,
}

impl SettlementLayerData {
    pub fn new(
        contracts: SettlementLayerContracts,
        l1_ecosystem_contracts: L1SpecificContracts,
    ) -> Self {
        Self {
            contracts,
            l1_ecosystem_contracts,
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
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerData {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "gateway_migrator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let mut contracts = self.contracts.clone();
        let initial_sl_mode = get_settlement_layer(
            &input.eth_client.0,
            self.contracts
                .l1_contracts()
                .chain_contracts_config
                .diamond_proxy_addr,
            &getters_facet_contract(),
        )
        .await?;
        contracts.set_settlement_mode(initial_sl_mode);
        let sl_chain_id = match initial_sl_mode {
            SettlementMode::SettlesToL1 => input
                .eth_client
                .0
                .fetch_chain_id()
                .await
                .context("fetch_chain_id")?,
            SettlementMode::Gateway => input
                .l2_eth_client
                .expect("Should be present for gateway settlement mode")
                .0
                .fetch_chain_id()
                .await
                .context("fetch_chain_id")?,
        };

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(initial_sl_mode),
            contracts: SettlementLayerContractsResource(contracts.current_contracts().clone()),
            l1_ecosystem_contracts: L1EcosystemContractsResource(
                self.l1_ecosystem_contracts.clone(),
            ),
            l1_contracts: L1ChainContractsResource(contracts.l1_contracts().clone()),
            sl_chain_id: SlChainIdResource(sl_chain_id),
        })
    }
}
