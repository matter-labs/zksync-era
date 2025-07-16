use anyhow::Context;
use zksync_basic_types::{
    pubdata_da::PubdataSendingMode,
    settlement::{SettlementLayer, WorkingSettlementLayer},
    url::SensitiveUrl,
    Address, L2ChainId, SLChainId,
};
use zksync_config::configs::{
    contracts::{
        chain::L2Contracts, ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts,
    },
    eth_sender::SenderConfig,
};
use zksync_contracts::getters_facet_contract;
use zksync_dal::{
    node::{MasterPool, PoolResource},
    Connection, Core, CoreDal,
};
use zksync_eth_client::{
    contracts_loader::{
        get_server_notifier_addr, get_settlement_layer_from_l1, get_zk_chain_on_chain_params,
        is_settlement_layer, load_settlement_layer_contracts,
    },
    node::SenderConfigResource,
    EthInterface,
};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::{
    contracts::{
        L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
        SettlementLayerContractsResource, ZkChainOnChainConfigResource,
    },
    PubdataSendingModeResource,
};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_web3_decl::{
    client::{Client, DynClient, L1, L2},
    namespaces::ZksNamespaceClient,
    node::{GatewayClientResource, SettlementLayerClient, SettlementModeResource},
};

use crate::{current_settlement_layer, gateway_urls};

pub struct MainNodeConfig {
    pub l1_specific_contracts: L1SpecificContracts,
    // This contracts are required as a fallback
    pub l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
    pub l2_contracts: L2Contracts,
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
pub struct Input {
    eth_client: Box<DynClient<L1>>,
    pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    initial_settlement_mode: SettlementModeResource,
    sl_client: SettlementLayerClient,
    gateway_client: Option<GatewayClientResource>,
    contracts: SettlementLayerContractsResource,
    l1_ecosystem_contracts: L1EcosystemContractsResource,
    l1_contracts: L1ChainContractsResource,
    l2_contracts: L2ContractsResource,
    zk_chain_on_chain_config: Option<ZkChainOnChainConfigResource>,
    eth_sender_config: Option<SenderConfigResource>,
    pubdata_sending_mode: Option<PubdataSendingModeResource>,
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
            &input.eth_client,
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
                get_server_notifier_addr(&input.eth_client, state_transition_proxy_addr)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
        }

        let l2_eth_client = get_l2_client(
            &input.eth_client,
            self.config
                .l1_specific_contracts
                .bridge_hub
                .expect("Bridge Hub should be always presented"),
            self.config.l2_chain_id,
            self.config.gateway_rpc_url,
        )
        .await?;

        let final_settlement_mode = current_settlement_layer(
            &input.eth_client,
            l2_eth_client
                .as_ref()
                .map(|client| client as &dyn EthInterface),
            &sl_l1_contracts,
            self.config.l2_chain_id,
            &getters_facet_contract(),
        )
        .await
        .context("error getting current SL mode")?;

        let sl_client = match final_settlement_mode.settlement_layer() {
            SettlementLayer::L1(_) => SettlementLayerClient::L1(input.eth_client),
            SettlementLayer::Gateway(_) => {
                // `unwrap()` is safe: `l2_eth_client` is always initialized when `config.gateway_rpc_url` is set,
                // which is required for `SettlementLayer::Gateway`.
                SettlementLayerClient::Gateway(l2_eth_client.clone().unwrap())
            }
        };

        let (sl_chain_contracts, zkchain_on_chain_config) = match &sl_client {
            SettlementLayerClient::L1(client) => {
                let zkchain_on_chain_config = get_zk_chain_on_chain_params(
                    client,
                    sl_l1_contracts.chain_contracts_config.diamond_proxy_addr,
                )
                .await
                .context("Chain config loading error")?;
                (sl_l1_contracts.clone(), zkchain_on_chain_config)
            }
            SettlementLayerClient::Gateway(client) => {
                let l2_multicall3 = client
                    .get_l2_multicall3()
                    .await
                    .context("Failed to fecth multicall3")?;

                let contracts = load_settlement_layer_contracts(
                    client,
                    L2_BRIDGEHUB_ADDRESS,
                    self.config.l2_chain_id,
                    l2_multicall3,
                )
                .await?
                // This unwrap is safe we have already verified it. Or it is supposed to be gateway,
                // but no gateway has been deployed
                .unwrap();

                let zkchain_on_chain_config = get_zk_chain_on_chain_params(
                    client,
                    contracts.chain_contracts_config.diamond_proxy_addr,
                )
                .await
                .context("Chain config loading error")?;
                (contracts, zkchain_on_chain_config)
            }
        };

        let eth_sender_config = adjust_eth_sender_config(
            self.config.eth_sender_config,
            final_settlement_mode.settlement_layer(),
        );

        Ok(Output {
            initial_settlement_mode: SettlementModeResource::new(final_settlement_mode.clone()),
            contracts: SettlementLayerContractsResource(sl_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(l1_specific_contracts),
            l1_contracts: L1ChainContractsResource(sl_l1_contracts),
            l2_contracts: L2ContractsResource(self.config.l2_contracts),
            pubdata_sending_mode: Some(PubdataSendingModeResource(
                eth_sender_config.pubdata_sending_mode,
            )),
            eth_sender_config: Some(SenderConfigResource(eth_sender_config)),
            sl_client,
            gateway_client: l2_eth_client.map(GatewayClientResource),
            zk_chain_on_chain_config: Some(ZkChainOnChainConfigResource(zkchain_on_chain_config)),
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

impl SettlementLayerData<ENConfig> {
    pub const LAYER_NAME: &'static str = "settlement_layer_en";
}

#[async_trait::async_trait]
impl WiringLayer for SettlementLayerData<ENConfig> {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        Self::LAYER_NAME
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let chain_id = input
            .eth_client
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
                &input.eth_client.as_ref(),
                self.config
                    .l1_chain_contracts
                    .chain_contracts_config
                    .diamond_proxy_addr,
                &getters_facet_contract(),
            )
            .await
            .context("Error occured while getting current SL mode")?
        };

        let l2_eth_client = get_l2_client(
            &input.eth_client,
            self.config
                .l1_specific_contracts
                .bridge_hub
                .expect("Bridge Hub should be always presented"),
            self.config.chain_id,
            self.config.gateway_rpc_url,
        )
        .await?;

        let (client, bridgehub): (&dyn EthInterface, Address) = match initial_sl_mode {
            SettlementLayer::L1(_) => (
                &input.eth_client,
                self.config
                    .l1_chain_contracts
                    .ecosystem_contracts
                    .bridgehub_proxy_addr
                    .context("missing `bridgehub_proxy_addr` in `l1_chain_contracts.ecosystem_contracts`")?,
            ),
            SettlementLayer::Gateway(_) => {
                (l2_eth_client.as_ref().unwrap(), L2_BRIDGEHUB_ADDRESS)
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

        let sl = WorkingSettlementLayer::new(initial_sl_mode);
        let sl_client = match sl.settlement_layer() {
            SettlementLayer::L1(_) => SettlementLayerClient::L1(input.eth_client),
            SettlementLayer::Gateway(_) => {
                // `unwrap()` is safe: `l2_eth_client` is always initialized when `config.gateway_rpc_url` is set,
                // which is required for `SettlementLayer::Gateway`.
                SettlementLayerClient::Gateway(
                    l2_eth_client
                        .clone()
                        .expect("Gateway rpc url is not presented"),
                )
            }
        };

        Ok(Output {
            initial_settlement_mode: SettlementModeResource::new(sl),
            sl_client,
            contracts: SettlementLayerContractsResource(contracts),
            l1_contracts: L1ChainContractsResource(self.config.l1_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(self.config.l1_specific_contracts),
            l2_contracts: L2ContractsResource(self.config.l2_contracts),
            gateway_client: l2_eth_client.map(GatewayClientResource),
            eth_sender_config: None,
            pubdata_sending_mode: None,
            zk_chain_on_chain_config: None,
        })
    }
}

async fn get_l2_client(
    eth_client: &dyn EthInterface,
    bridgehub_address: Address,
    l2_chain_id: L2ChainId,
    gateway_rpc_url: Option<SensitiveUrl>,
) -> anyhow::Result<Option<Box<DynClient<L2>>>> {
    // If the server is the settlement layer, the gateway is the server itself,
    // so we can't point to ourselves.
    let is_settlement_layer = is_settlement_layer(eth_client, bridgehub_address, l2_chain_id)
        .await
        .context("failed to call whitelistedSettlementLayers on the bridgehub contract")?;

    if !is_settlement_layer {
        get_l2_client_unchecked(gateway_rpc_url, bridgehub_address).await
    } else {
        Ok(None)
    }
}

async fn get_l2_client_unchecked(
    gateway_rpc_url: Option<SensitiveUrl>,
    l1_bridgehub_address: Address,
) -> anyhow::Result<Option<Box<DynClient<L2>>>> {
    // If gateway rpc is not presented try to fallback to the default gateway url
    let gateway_rpc_url = if let Some(url) = gateway_rpc_url {
        Some(url)
    } else {
        gateway_urls::DefaultGatewayUrl::from_bridgehub_address(l1_bridgehub_address)
            .map(|a| a.to_gateway_url())
    };
    Ok(if let Some(url) = gateway_rpc_url {
        let client: Client<L2> = Client::http(url.clone()).context("Client::new()")?.build();
        let chain_id = client.fetch_chain_id().await?;
        let client = Client::http(url)
            .context("Client::new()")?
            .for_network(L2ChainId::new(chain_id.0).unwrap().into())
            .build();
        Some(Box::new(client))
    } else {
        tracing::warn!(
            "No client was found for gateway, you are working in none \
            ZkSync ecosystem and haven't specified secret for gateway. During the migration it could cause a downtime"
        );
        None
    })
}

// Gateway has different rules for pubdata and gas space.
// We need to adjust it accordingly.
fn adjust_eth_sender_config(
    mut config: SenderConfig,
    settlement_layer: SettlementLayer,
) -> SenderConfig {
    if settlement_layer.is_gateway() {
        config.max_aggregated_tx_gas = 30000000000;
        tracing::warn!(
            "Settling to Gateway requires to adjust ETH sender configs: \
               max_aggregated_tx_gas = {}",
            config.max_aggregated_tx_gas
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

// Get settlement layer based on ETH tx in the database. We start on SL matching oldest unfinalized eth tx.
// This due to BatchTransactionUpdater needing this SL to finalize that batch transaction.
async fn get_db_settlement_mode(
    connection: &mut Connection<'_, Core>,
    l1chain_id: SLChainId,
) -> anyhow::Result<Option<SettlementLayer>> {
    let db_chain_id = connection
        .eth_sender_dal()
        .get_chain_id_of_oldest_unfinalized_eth_tx()
        .await?;

    Ok(db_chain_id.map(|chain_id| {
        if chain_id != l1chain_id.0 {
            SettlementLayer::Gateway(SLChainId(chain_id))
        } else {
            SettlementLayer::L1(SLChainId(chain_id))
        }
    }))
}
