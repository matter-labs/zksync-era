use std::sync::Arc;

use anyhow::Context;
use zksync_config::configs::{
    contracts::{
        chain::L2Contracts, ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts,
    },
    eth_sender::SenderConfig,
};
use zksync_consistency_checker::get_db_settlement_mode;
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::{
    contracts_loader::{get_settlement_layer_from_l1, load_settlement_layer_contracts},
    EthInterface,
};
use zksync_gateway_migrator::current_settlement_mode;
use zksync_types::{
    pubdata_da::PubdataSendingMode, settlement::SettlementLayer, url::SensitiveUrl, Address,
    L2ChainId, L2_BRIDGEHUB_ADDRESS,
};
use zksync_web3_decl::client::Client;

use crate::{
    implementations::resources::{
        contracts::{
            L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
            SettlementLayerContractsResource,
        },
        eth_interface::{EthInterfaceResource, L2InterfaceResource},
        pools::{MasterPool, PoolResource},
        settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

pub struct MainNodeConfig {
    l2_contracts: L2Contracts,
    l1_specific_contracts: L1SpecificContracts,
    // This contracts are required as a fallback
    l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
    l2_chain_id: L2ChainId,
    multicall3: Option<Address>,
    gateway_rpc_url: Option<SensitiveUrl>,
    eth_sender_config: SenderConfig,
}

impl MainNodeConfig {
    pub fn new(
        l1_specific_contracts: L1SpecificContracts,
        l2_contracts: L2Contracts,
        l2_chain_id: L2ChainId,
        multicall3: Option<Address>,
        l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
        gateway_rpc_url: Option<SensitiveUrl>,
        eth_sender_config: SenderConfig,
    ) -> Self {
        Self {
            l2_contracts,
            l1_specific_contracts,
            l1_sl_specific_contracts,
            l2_chain_id,
            multicall3,
            gateway_rpc_url,
            eth_sender_config,
        }
    }
}

/// Wiring layer for [`SettlementLayerData`].
#[derive(Debug)]
pub struct SettlementLayerData<T> {
    config: T,
}

impl<T> SettlementLayerData<T> {
    pub fn new(config: T) -> Self {
        Self { config }
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
    l2_contracts: L2ContractsResource,
    l2_eth_client: Option<L2InterfaceResource>,
    eth_sender_config: Option<SenderConfig>,
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerData<MainNodeConfig> {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "settlement_layer_data"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let sl_l1_contracts = load_settlement_layer_contracts(
            &input.eth_client.0,
            self.config.l1_specific_contracts.bridge_hub.unwrap(),
            self.config.l2_chain_id,
            self.config.multicall3,
        )
        .await?
        // before v26 upgrade not all function for getting addresses are available,
        // so we need a fallback and we can load the contracts from configs,
        // it's safe only for l1 contracts
        .unwrap_or(self.config.l1_sl_specific_contracts.unwrap());

        let pool = input.pool.get().await?;

        let l2_eth_client = get_l2_client(self.config.gateway_rpc_url)?;
        let gateway_client: Option<Arc<dyn EthInterface>> = l2_eth_client
            .clone()
            .map(|a| Arc::new(a.0) as Arc<dyn EthInterface>);

        let (final_settlement_mode, sl_chain_contracts) = current_settlement_mode(
            &input.eth_client.0,
            gateway_client,
            &sl_l1_contracts,
            self.config.l2_chain_id,
            &mut pool.connection().await.context("Can't connect")?,
            &getters_facet_contract(),
        )
        .await?;

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(final_settlement_mode),
            contracts: SettlementLayerContractsResource(sl_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(
                self.config.l1_specific_contracts.clone(),
            ),
            l2_contracts: L2ContractsResource(self.config.l2_contracts),
            l1_contracts: L1ChainContractsResource(sl_l1_contracts),
            l2_eth_client,
            eth_sender_config: Some(adjust_eth_sender_config(
                self.config.eth_sender_config,
                final_settlement_mode,
            )),
        })
    }
}

#[derive(Debug)]
pub struct ENConfig {
    l1_specific_contracts: L1SpecificContracts,
    l1_chain_contracts: SettlementLayerSpecificContracts,
    l2_contracts: L2Contracts,
    chain_id: L2ChainId,
    gateway_rpc_url: Option<SensitiveUrl>,
}

impl ENConfig {
    pub fn new(
        chain_id: L2ChainId,
        l1_specific_contracts: L1SpecificContracts,
        l1_chain_contracts: SettlementLayerSpecificContracts,
        l2_contracts: L2Contracts,
        gateway_rpc_url: Option<SensitiveUrl>,
    ) -> Self {
        Self {
            l1_specific_contracts,
            l1_chain_contracts,
            l2_contracts,
            chain_id,
            gateway_rpc_url,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerData<ENConfig> {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "settlement_layer_en"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let chain_id = input
            .eth_client
            .0
            .fetch_chain_id()
            .await
            .context("Problem with fetching chain id")?;

        let initial_db_sl_mode = get_db_settlement_mode(input.pool.get().await?, chain_id).await?;

        let initial_sl_mode = if let Some(mode) = initial_db_sl_mode {
            mode
        } else {
            // If it's the new chain it's safe to check the actual sl onchain,
            // in the worst case scenario chain
            // en will be restarted right after the first batch and fill the database with correct values
            get_settlement_layer_from_l1(
                &input.eth_client.0.as_ref(),
                self.config
                    .l1_chain_contracts
                    .chain_contracts_config
                    .diamond_proxy_addr,
                &getters_facet_contract(),
            )
            .await?
        };

        let l2_eth_client = get_l2_client(self.config.gateway_rpc_url)?;

        let (client, bridgehub): (&dyn EthInterface, Address) = match initial_sl_mode {
            SettlementLayer::L1(_) => (
                &input.eth_client.0,
                self.config
                    .l1_chain_contracts
                    .ecosystem_contracts
                    .bridgehub_proxy_addr
                    .unwrap(),
            ),
            SettlementLayer::Gateway(_) => {
                (&l2_eth_client.as_ref().unwrap().0, L2_BRIDGEHUB_ADDRESS)
            }
        };

        // There is no need to specify multicall3 for external node
        let contracts =
            load_settlement_layer_contracts(client, bridgehub, self.config.chain_id, None).await?;
        let contracts = match contracts {
            Some(contracts) => contracts,
            None => match initial_sl_mode {
                SettlementLayer::L1(_) => self.config.l1_chain_contracts.clone(),
                SettlementLayer::Gateway(_) => {
                    return Err(anyhow::anyhow!("No contacts deployed to contracts"))?
                }
            },
        };

        Ok(Output {
            contracts: SettlementLayerContractsResource(contracts),
            l1_contracts: L1ChainContractsResource(self.config.l1_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(self.config.l1_specific_contracts),
            l2_contracts: L2ContractsResource(self.config.l2_contracts),
            initial_settlement_mode: SettlementModeResource(initial_sl_mode),
            l2_eth_client,
            eth_sender_config: None,
        })
    }
}

fn get_l2_client(
    gateway_rpc_url: Option<SensitiveUrl>,
) -> anyhow::Result<Option<L2InterfaceResource>> {
    Ok(gateway_rpc_url
        .map(|url| Client::http(url).context("Client::new()"))
        .transpose()?
        .map(|builder| L2InterfaceResource(Box::new(builder.build()))))
}

// Gateway has different rules for pubdata and gas space.
// We need to adjust it accordingly.
fn adjust_eth_sender_config(
    mut config: SenderConfig,
    settlement_layer: SettlementLayer,
) -> SenderConfig {
    if settlement_layer.is_gateway() {
        config.max_aggregated_tx_gas = 4294967295;
        config.max_eth_tx_data_size = 550_000;
        tracing::warn!(
            "Settling to Gateway requires to adjust ETH sender configs: \
               max_aggregated_tx_gas = {}, max_eth_tx_data_size = {}",
            config.max_aggregated_tx_gas,
            config.max_eth_tx_data_size
        );
        if config.pubdata_sending_mode == PubdataSendingMode::Blobs
            || config.pubdata_sending_mode == PubdataSendingMode::Calldata
        {
            tracing::warn!(
                "Settling to Gateway requires to adjust Pub Data Sending Mode: \
                    changed from {:?} to {:?} ",
                &config.pubdata_sending_mode,
                PubdataSendingMode::RelayedL2Calldata
            );
            config.pubdata_sending_mode = PubdataSendingMode::RelayedL2Calldata;
        }
    }
    config
}
