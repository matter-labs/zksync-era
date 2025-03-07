use zksync_config::configs::contracts::{ecosystem::L1SpecificContracts, ChainSpecificContracts};
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
pub struct SettlementLayerDataEn {
    l1specific_contracts: L1SpecificContracts,
    chain_specific_contracts: ChainSpecificContracts,
}

impl SettlementLayerDataEn {
    pub fn new(
        l1specific_contracts: L1SpecificContracts,
        chain_specific_contracts: ChainSpecificContracts,
    ) -> Self {
        Self {
            l1specific_contracts,
            chain_specific_contracts,
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
    l1_contracts: L1ChainContractsResource,
    l1_ecosystem_contracts: L1EcosystemContractsResource,
    sl_chain_id_resource: SlChainIdResource,
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerDataEn {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "settlement_layer_en"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let initial_sl_mode = get_settlement_layer(
            &input.eth_client.0,
            self.chain_specific_contracts
                .chain_contracts_config
                .diamond_proxy_addr,
            &getters_facet_contract(),
        )
        .await?;

        let chain_id = match initial_sl_mode {
            SettlementMode::SettlesToL1 => input.eth_client.0.fetch_chain_id().await.unwrap(),
            SettlementMode::Gateway => input
                .l2_eth_client
                .unwrap()
                .0
                .fetch_chain_id()
                .await
                .unwrap(),
        };

        Ok(Output {
            contracts: SettlementLayerContractsResource(self.chain_specific_contracts.clone()),
            l1_contracts: L1ChainContractsResource(self.chain_specific_contracts.clone()),
            l1_ecosystem_contracts: L1EcosystemContractsResource(self.l1specific_contracts.clone()),
            initial_settlement_mode: SettlementModeResource(initial_sl_mode),
            sl_chain_id_resource: SlChainIdResource(chain_id),
        })
    }
}
