use zksync_config::Contracts;
use zksync_contracts::getters_facet_contract;
use zksync_gateway_migrator::get_settlement_layer;

use crate::{
    implementations::resources::{
        contracts::ContractsResource, eth_interface::EthInterfaceResource,
        settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`SettlementLayerData`].
#[derive(Debug)]
pub struct SettlementLayerData {
    contracts: Contracts,
}

impl SettlementLayerData {
    pub fn new(contracts_config: Contracts) -> Self {
        Self {
            contracts: contracts_config,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    initial_settlement_mode: SettlementModeResource,
    contracts: ContractsResource,
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

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(initial_sl_mode),
            contracts: ContractsResource(contracts),
        })
    }
}
