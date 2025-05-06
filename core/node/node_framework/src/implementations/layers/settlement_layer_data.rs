use anyhow::Context;
use zksync_config::configs::{
    contracts::{
        chain::L2Contracts, ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts,
    },
    eth_sender::SenderConfig,
};
use zksync_contracts::getters_facet_contract;
use zksync_dal::{Core, CoreDal};
use zksync_db_connection::connection::Connection;
use zksync_eth_client::{
    contracts_loader::{
        get_server_notifier_addr, get_settlement_layer_from_l1, load_settlement_layer_contracts,
    },
    EthInterface,
};
use zksync_gateway_migrator::current_settlement_layer;
use zksync_types::{
    pubdata_da::PubdataSendingMode, settlement::SettlementLayer, url::SensitiveUrl, Address,
    L2ChainId, SLChainId, L2_BRIDGEHUB_ADDRESS,
};
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::ZksNamespaceClient,
};

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
    pub l2_contracts: L2Contracts,
    pub l1_specific_contracts: L1SpecificContracts,
    // This contracts are required as a fallback
    pub l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
    pub l2_chain_id: L2ChainId,
    pub multicall3: Option<Address>,
    pub gateway_rpc_url: Option<SensitiveUrl>,
    pub eth_sender_config: SenderConfig,
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

        let mut l1_specific_contracts = self.config.l1_specific_contracts.clone();
        // In the future we will be able to load all contracts from the l1 chain. Now for not adding
        // new config variable we are loading only the server notifier address
        if l1_specific_contracts.server_notifier_addr.is_none() {
            l1_specific_contracts.server_notifier_addr = if let Some(state_transition_proxy_addr) =
                sl_l1_contracts
                    .ecosystem_contracts
                    .state_transition_proxy_addr
            {
                get_server_notifier_addr(&input.eth_client.0, state_transition_proxy_addr)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
        }

        let pool = input.pool.get().await?;

        let l2_eth_client = get_l2_client(self.config.gateway_rpc_url).await?;

        let final_settlement_mode = current_settlement_layer(
            &input.eth_client.0,
            l2_eth_client.as_ref().map(|a| &a.0 as &dyn EthInterface),
            &sl_l1_contracts,
            self.config.l2_chain_id,
            &mut pool.connection().await.context("Can't connect")?,
            &getters_facet_contract(),
        )
        .await
        .context("Error occured while getting current SL mode")?;

        let sl_chain_contracts = match final_settlement_mode {
            SettlementLayer::L1(_) => sl_l1_contracts.clone(),
            SettlementLayer::Gateway(_) => {
                let client = l2_eth_client.clone().unwrap().0;
                let l2_multicall3 = client
                    .get_l2_multicall3()
                    .await
                    .context("Failed to fecth multicall3")?;

                load_settlement_layer_contracts(
                    &client,
                    L2_BRIDGEHUB_ADDRESS,
                    self.config.l2_chain_id,
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
            l1_ecosystem_contracts: L1EcosystemContractsResource(l1_specific_contracts),
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
    pub l1_specific_contracts: L1SpecificContracts,
    pub l1_chain_contracts: SettlementLayerSpecificContracts,
    pub l2_contracts: L2Contracts,
    pub chain_id: L2ChainId,
    pub gateway_rpc_url: Option<SensitiveUrl>,
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

        let initial_db_sl_mode = get_db_settlement_mode(
            &mut input
                .pool
                .get()
                .await?
                .connection()
                .await
                .context("failed getting pool connection")?,
            chain_id,
        )
        .await?;

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
            .await
            .context("Error occured while getting current SL mode")?
        };

        let l2_eth_client = get_l2_client(self.config.gateway_rpc_url).await?;

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

async fn get_l2_client(
    gateway_rpc_url: Option<SensitiveUrl>,
) -> anyhow::Result<Option<L2InterfaceResource>> {
    let res = if let Some(url) = gateway_rpc_url {
        let client: Client<L2> = Client::http(url.clone()).context("Client::new()")?.build();
        let chain_id = client.fetch_chain_id().await?;
        Some(L2InterfaceResource(Box::new(
            Client::http(url)
                .context("Client::new()")?
                .for_network(L2ChainId::new(chain_id.0).unwrap().into())
                .build(),
        )))
    } else {
        None
    };
    Ok(res)
}

// Gateway has different rules for pubdata and gas space.
// We need to adjust it accordingly.
fn adjust_eth_sender_config(
    mut config: SenderConfig,
    settlement_layer: SettlementLayer,
) -> SenderConfig {
    if settlement_layer.is_gateway() {
        config.max_aggregated_tx_gas = 30000000000;
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

// Get settlement layer based on ETH tx in the database.
async fn get_db_settlement_mode(
    connection: &mut Connection<'_, Core>,
    l1chain_id: SLChainId,
) -> anyhow::Result<Option<SettlementLayer>> {
    let db_chain_id = connection
        .eth_sender_dal()
        .get_chain_id_of_last_eth_tx()
        .await?;

    Ok(db_chain_id.map(|chain_id| {
        if chain_id != l1chain_id.0 {
            SettlementLayer::Gateway(SLChainId(chain_id))
        } else {
            SettlementLayer::L1(SLChainId(chain_id))
        }
    }))
}
