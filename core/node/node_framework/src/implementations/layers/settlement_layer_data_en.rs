use zksync_config::configs::contracts::{ecosystem::L1SpecificContracts, ChainSpecificContracts};
use zksync_consistency_checker::get_settlement_mode;
use zksync_eth_client::EthInterface;
use zksync_types::settlement::SettlementMode;

use crate::{
    implementations::resources::{
        contracts::{
            L1ChainContractsResource, L1EcosystemContractsResource,
            SettlementLayerContractsResource,
        },
        eth_interface::{EthInterfaceResource, L2InterfaceResource},
        pools::{MasterPool, PoolResource},
        settlement_layer::{SettlementModeResource, SlChainIdResource},
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`SettlementLayerData`].
#[derive(Debug)]
pub struct SettlementLayerDataEn {
    l1_specific_contracts: L1SpecificContracts,
    sl_chain_contracts: ChainSpecificContracts,
    l1_chain_contracts: ChainSpecificContracts,
}

impl SettlementLayerDataEn {
    pub fn new(
        l1_specific_contracts: L1SpecificContracts,
        sl_chain_contracts: ChainSpecificContracts,
        l1_chain_contracts: ChainSpecificContracts,
    ) -> Self {
        Self {
            l1_specific_contracts,
            sl_chain_contracts,
            l1_chain_contracts,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub l2_eth_client: Option<L2InterfaceResource>,
    pub master_pool: PoolResource<MasterPool>,
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
        let initial_sl_mode = get_settlement_mode(
            input.master_pool.get().await?,
            input.eth_client.0.clone(),
            input.l2_eth_client.as_ref().map(|a| a.0.clone()),
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
            contracts: SettlementLayerContractsResource(self.sl_chain_contracts.clone()),
            l1_contracts: L1ChainContractsResource(self.l1_chain_contracts.clone()),
            l1_ecosystem_contracts: L1EcosystemContractsResource(
                self.l1_specific_contracts.clone(),
            ),
            initial_settlement_mode: SettlementModeResource(initial_sl_mode),
            sl_chain_id_resource: SlChainIdResource(chain_id),
        })
    }
}
