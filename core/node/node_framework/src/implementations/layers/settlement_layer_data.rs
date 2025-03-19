use anyhow::Context;
use zksync_config::configs::contracts::{
    chain::L2Contracts, ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts,
};
use zksync_contracts::getters_facet_contract;
use zksync_contracts_loader::{get_settlement_layer_from_l1, load_settlement_layer_contracts};
use zksync_eth_client::EthInterface;
use zksync_gateway_migrator::switch_to_current_settlement_mode;
use zksync_types::{
    settlement::SettlementMode, url::SensitiveUrl, Address, L2ChainId, L2_BRIDGEHUB_ADDRESS,
};
use zksync_web3_decl::{client::Client, namespaces::ZksNamespaceClient};

use crate::{
    implementations::resources::{
        contracts::{
            L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
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
pub struct SettlementLayerData {
    l2_contracts: L2Contracts,
    l1_specific_contracts: L1SpecificContracts,
    // This contracts are required as a fallback
    l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
    gateway_rpc_url: Option<SensitiveUrl>,
}

impl SettlementLayerData {
    pub fn new(
        l1_specific_contracts: L1SpecificContracts,
        l2_contracts: L2Contracts,
        l2_chain_id: L2ChainId,
        multicall3: Option<Address>,
        l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
        gateway_rpc_url: Option<SensitiveUrl>,
    ) -> Self {
        Self {
            l2_contracts,
            l1_specific_contracts,
            l1_sl_specific_contracts,
            l2_chain_id,
            multicall3,
            gateway_rpc_url,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
    pub pool: PoolResource<MasterPool>,
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
    l2_eth_client: Option<L2InterfaceResource>,
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerData {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "settlement_layer_data"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let sl_l1_contracts = load_settlement_layer_contracts(
            &input.eth_client.0,
            self.l1_specific_contracts.bridge_hub.unwrap(),
            self.l2_chain_id,
            self.multicall3,
        )
        .await?
        // before v26 upgrade not all function for getting addresses are available,
        // so we need a fallback and we can load the contracts from configs,
        // it's safe only for l1 contracts
        .unwrap_or(self.l1_sl_specific_contracts.unwrap());

        let (initial_sl_mode, sl_chain_id) = get_settlement_layer_from_l1(
            &input.eth_client.0,
            sl_l1_contracts.chain_contracts_config.diamond_proxy_addr,
            &getters_facet_contract(),
        )
        .await?;

        let pool = input.pool.get().await?;

        let l2_eth_client = self
            .gateway_rpc_url
            .map(|url| Client::http(url).context("Client::new()"))
            .transpose()?
            .and_then(|builder| {
                initial_sl_mode.is_gateway().then(|| {
                    Some(L2InterfaceResource(Box::new(
                        builder
                            .for_network(L2ChainId::new(sl_chain_id.0).unwrap().into())
                            .build(),
                    )))
                })
            })
            .flatten();

        let (bridgehub, sl_client): (_, &dyn EthInterface) = match initial_sl_mode {
            SettlementMode::SettlesToL1 => (
                self.l1_specific_contracts.bridge_hub.expect("must exist"),
                &input.eth_client.0,
            ),
            SettlementMode::Gateway => (L2_BRIDGEHUB_ADDRESS, &l2_eth_client.as_ref().unwrap().0),
        };

        let switch = switch_to_current_settlement_mode(
            initial_sl_mode,
            sl_client,
            self.l2_chain_id,
            &mut pool.connection().await.context("Can't connect")?,
            bridgehub,
            &getters_facet_contract(),
        )
        .await?;

        let final_settlement_mode = if switch {
            initial_sl_mode
        } else {
            match initial_sl_mode {
                SettlementMode::SettlesToL1 => SettlementMode::Gateway,
                SettlementMode::Gateway => SettlementMode::SettlesToL1,
            }
        };

        let sl_chain_contracts = match final_settlement_mode {
            SettlementMode::SettlesToL1 => sl_l1_contracts.clone(),
            SettlementMode::Gateway => {
                let client = l2_eth_client.clone().unwrap().0;
                let l2_multicall3 = client
                    .get_l2_multicall3()
                    .await
                    .context("Failed to fecth multicall3")?;

                load_settlement_layer_contracts(
                    &client,
                    L2_BRIDGEHUB_ADDRESS,
                    self.l2_chain_id,
                    l2_multicall3,
                )
                .await?
                // This unwrap is safe we have already verified it. Or it is supposed to be gateway,
                // but no gateway has been deployed
                .unwrap()
            }
        };

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(final_settlement_mode),
            contracts: SettlementLayerContractsResource(sl_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(
                self.l1_specific_contracts.clone(),
            ),
            l2_contracts: L2ContractsResource(self.l2_contracts),
            l1_contracts: L1ChainContractsResource(sl_l1_contracts),
            sl_chain_id: SlChainIdResource(sl_chain_id),
            l2_eth_client,
        })
    }
}
