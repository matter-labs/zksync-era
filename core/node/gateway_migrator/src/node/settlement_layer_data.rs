use std::future::Future;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    pubdata_da::PubdataSendingMode,
    settlement::{SettlementLayer, WorkingSettlementLayer},
    url::SensitiveUrl,
    Address, L2ChainId, SLChainId,
};
use zksync_config::configs::{
    contracts::{
        chain::{ChainContracts, L2Contracts},
        ecosystem::{EcosystemCommonContracts, L1SpecificContracts},
        SettlementLayerSpecificContracts,
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
        get_server_notifier_addr, get_settlement_layer_from_l1, is_settlement_layer,
        load_settlement_layer_contracts,
    },
    node::SenderConfigResource,
    ClientError, EthInterface,
};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::{
    contracts::{
        L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
        SettlementLayerContractsResource,
    },
    PubdataSendingModeResource,
};
use zksync_system_constants::{ETHEREUM_ADDRESS, L2_BRIDGEHUB_ADDRESS};
use zksync_web3_decl::{
    client::{Client, DynClient, L1, L2},
    error::ClientRpcContext,
    jsonrpsee::types::ErrorCode,
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
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

        let sl_chain_contracts = match &sl_client {
            SettlementLayerClient::L1(_) => sl_l1_contracts.clone(),
            SettlementLayerClient::Gateway(client) => {
                let l2_multicall3 = client
                    .get_l2_multicall3()
                    .await
                    .context("Failed to fecth multicall3")?;

                load_settlement_layer_contracts(
                    client,
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
        })
    }
}

#[derive(Debug)]
pub struct ENConfig {
    pub chain_id: L2ChainId,
    pub gateway_rpc_url: Option<SensitiveUrl>,
    pub main_node_url: SensitiveUrl,
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
        let main_node_client = Client::http(self.config.main_node_url.clone())
            .context("failed creating JSON-RPC client for main node")?
            .for_network(self.config.chain_id.into())
            .build();
        let main_node_client = Box::new(main_node_client) as Box<DynClient<L2>>;
        let remote_config = RemoteENConfig::fetch(main_node_client).await?;

        let initial_sl_mode = if let Some(mode) = initial_db_sl_mode {
            mode
        } else {
            // If it's the new chain it's safe to check the actual sl onchain,
            // in the worst case scenario chain
            // en will be restarted right after the first batch and fill the database with correct values
            get_settlement_layer_from_l1(
                &input.eth_client.as_ref(),
                remote_config.l1_diamond_proxy_addr,
                &getters_facet_contract(),
            )
            .await
            .context("Error occured while getting current SL mode")?
        };

        let l2_eth_client = get_l2_client(
            &input.eth_client,
            remote_config.l1_bridgehub_proxy_addr.unwrap(),
            self.config.chain_id,
            self.config.gateway_rpc_url,
        )
        .await?;

        let (client, bridgehub): (&dyn EthInterface, Address) = match initial_sl_mode {
            SettlementLayer::L1(_) => (
                &input.eth_client,
                remote_config.l1_bridgehub_proxy_addr.context(
                    "missing `bridgehub_proxy_addr` in `l1_chain_contracts.ecosystem_contracts`",
                )?,
            ),
            SettlementLayer::Gateway(_) => (l2_eth_client.as_ref().unwrap(), L2_BRIDGEHUB_ADDRESS),
        };

        // There is no need to specify multicall3 for external node
        let contracts =
            load_settlement_layer_contracts(client, bridgehub, self.config.chain_id, None).await?;
        let contracts = match contracts {
            Some(contracts) => contracts,
            None => match initial_sl_mode {
                SettlementLayer::L1(_) => remote_config.l1_settelment_contracts().clone(),
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
            l1_contracts: L1ChainContractsResource(remote_config.l1_settelment_contracts()),
            l1_ecosystem_contracts: L1EcosystemContractsResource(
                remote_config.l1_specific_contracts(),
            ),
            l2_contracts: L2ContractsResource(remote_config.l2_contracts()),
            gateway_client: l2_eth_client.map(GatewayClientResource),
            eth_sender_config: None,
            pubdata_sending_mode: None,
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

/// This part of the external node config is fetched directly from the main node.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RemoteENConfig {
    pub l1_bytecodes_supplier_addr: Option<Address>,
    pub l1_bridgehub_proxy_addr: Option<Address>,
    pub l1_state_transition_proxy_addr: Option<Address>,
    /// Should not be accessed directly. Use [`ExternalNodeConfig::l1_diamond_proxy_address`] instead.
    l1_diamond_proxy_addr: Address,
    // While on L1 shared bridge and legacy bridge are different contracts with different addresses,
    // the `l2_erc20_bridge_addr` and `l2_shared_bridge_addr` are basically the same contract, but with
    // a different name, with names adapted only for consistency.
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    /// Contract address that serves as a shared bridge on L2.
    /// It is expected that `L2SharedBridge` is used before gateway upgrade, and `L2AssetRouter` is used after.
    pub l2_shared_bridge_addr: Address,
    /// Address of `L2SharedBridge` that was used before gateway upgrade.
    /// `None` if chain genesis used post-gateway protocol version.
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    pub l1_erc20_bridge_proxy_addr: Option<Address>,
    pub l2_erc20_bridge_addr: Address,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l2_timestamp_asserter_addr: Option<Address>,
    pub l1_wrapped_base_token_store: Option<Address>,
    pub l1_server_notifier_addr: Option<Address>,
    pub l1_message_root_proxy_addr: Option<Address>,
    pub base_token_addr: Address,
    pub l2_multicall3: Option<Address>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub dummy_verifier: bool,
}

impl RemoteENConfig {
    pub async fn fetch(client: Box<DynClient<L2>>) -> anyhow::Result<Self> {
        let bridges = client
            .get_bridge_contracts()
            .rpc_context("get_bridge_contracts")
            .await?;
        let l2_testnet_paymaster_addr = client
            .get_testnet_paymaster()
            .rpc_context("get_testnet_paymaster")
            .await?;
        let genesis = client.genesis_config().rpc_context("genesis").await.ok();

        let l1_ecosystem_contracts = client
            .get_ecosystem_contracts()
            .rpc_context("l1_ecosystem_contracts")
            .await
            .ok();
        let l1_diamond_proxy_addr = client
            .get_main_l1_contract()
            .rpc_context("get_main_l1_contract")
            .await?;

        let timestamp_asserter_address = handle_rpc_response_with_fallback(
            client.get_timestamp_asserter(),
            None,
            "Failed to fetch timestamp asserter address".to_string(),
        )
        .await?;
        let l2_multicall3 = handle_rpc_response_with_fallback(
            client.get_l2_multicall3(),
            None,
            "Failed to fetch l2 multicall3".to_string(),
        )
        .await?;
        let base_token_addr = handle_rpc_response_with_fallback(
            client.get_base_token_l1_address(),
            ETHEREUM_ADDRESS,
            "Failed to fetch base token address".to_string(),
        )
        .await?;

        let l2_erc20_default_bridge = bridges
            .l2_erc20_default_bridge
            .or(bridges.l2_shared_default_bridge)
            .unwrap();
        let l2_erc20_shared_bridge = bridges
            .l2_shared_default_bridge
            .or(bridges.l2_erc20_default_bridge)
            .unwrap();

        if l2_erc20_default_bridge != l2_erc20_shared_bridge {
            panic!("L2 erc20 bridge address and L2 shared bridge address are different.");
        }

        Ok(Self {
            l1_bridgehub_proxy_addr: l1_ecosystem_contracts
                .as_ref()
                .map(|a| a.bridgehub_proxy_addr),
            l1_state_transition_proxy_addr: l1_ecosystem_contracts
                .as_ref()
                .and_then(|a| a.state_transition_proxy_addr),
            l1_bytecodes_supplier_addr: l1_ecosystem_contracts
                .as_ref()
                .and_then(|a| a.l1_bytecodes_supplier_addr),
            l1_wrapped_base_token_store: l1_ecosystem_contracts
                .as_ref()
                .and_then(|a| a.l1_wrapped_base_token_store),
            l1_server_notifier_addr: l1_ecosystem_contracts
                .as_ref()
                .and_then(|a| a.server_notifier_addr),
            l1_diamond_proxy_addr,
            l2_testnet_paymaster_addr,
            l1_erc20_bridge_proxy_addr: bridges.l1_erc20_default_bridge,
            l2_erc20_bridge_addr: l2_erc20_default_bridge,
            l1_shared_bridge_proxy_addr: bridges.l1_shared_default_bridge,
            l2_shared_bridge_addr: l2_erc20_shared_bridge,
            l2_legacy_shared_bridge_addr: bridges.l2_legacy_shared_bridge,
            base_token_addr,
            l2_multicall3,
            l1_batch_commit_data_generator_mode: genesis
                .as_ref()
                .map(|a| a.l1_batch_commit_data_generator_mode)
                .unwrap_or_default(),
            dummy_verifier: genesis
                .as_ref()
                .map(|a| a.dummy_verifier)
                .unwrap_or_default(),
            l2_timestamp_asserter_addr: timestamp_asserter_address,
            l1_message_root_proxy_addr: l1_ecosystem_contracts
                .as_ref()
                .and_then(|a| a.message_root_proxy_addr),
        })
    }

    #[cfg(test)]
    fn mock() -> Self {
        Self {
            l1_bytecodes_supplier_addr: None,
            l1_bridgehub_proxy_addr: Some(Address::repeat_byte(8)),
            l1_state_transition_proxy_addr: None,
            l1_diamond_proxy_addr: Address::repeat_byte(1),
            l1_erc20_bridge_proxy_addr: Some(Address::repeat_byte(2)),
            l2_erc20_bridge_addr: Address::repeat_byte(3),
            l2_testnet_paymaster_addr: None,
            base_token_addr: Address::repeat_byte(4),
            l1_shared_bridge_proxy_addr: Some(Address::repeat_byte(5)),
            l2_shared_bridge_addr: Address::repeat_byte(6),
            l2_legacy_shared_bridge_addr: Some(Address::repeat_byte(7)),
            l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
            l1_wrapped_base_token_store: None,
            dummy_verifier: true,
            l2_timestamp_asserter_addr: None,
            l1_server_notifier_addr: None,
            l2_multicall3: None,
            l1_message_root_proxy_addr: None,
        }
    }

    pub fn l1_specific_contracts(&self) -> L1SpecificContracts {
        L1SpecificContracts {
            bytecodes_supplier_addr: self.l1_bytecodes_supplier_addr,
            wrapped_base_token_store: self.l1_wrapped_base_token_store,
            bridge_hub: self.l1_bridgehub_proxy_addr,
            shared_bridge: self.l1_shared_bridge_proxy_addr,
            message_root: self.l1_message_root_proxy_addr,
            erc_20_bridge: self.l1_erc20_bridge_proxy_addr,
            base_token_address: self.base_token_addr,
            server_notifier_addr: self.l1_server_notifier_addr,
            // We don't need chain admin for external node
            chain_admin: None,
        }
    }

    pub fn l1_settelment_contracts(&self) -> SettlementLayerSpecificContracts {
        SettlementLayerSpecificContracts {
            ecosystem_contracts: EcosystemCommonContracts {
                bridgehub_proxy_addr: self.l1_bridgehub_proxy_addr,
                state_transition_proxy_addr: self.l1_state_transition_proxy_addr,
                message_root_proxy_addr: self.l1_message_root_proxy_addr,
                // Multicall 3 is useless for external node
                multicall3: None,
                validator_timelock_addr: None,
            },
            chain_contracts_config: ChainContracts {
                diamond_proxy_addr: self.l1_diamond_proxy_addr,
            },
        }
    }

    pub fn l2_contracts(&self) -> L2Contracts {
        L2Contracts {
            erc20_default_bridge: self.l2_erc20_bridge_addr,
            shared_bridge_addr: self.l2_shared_bridge_addr,
            legacy_shared_bridge_addr: self.l2_legacy_shared_bridge_addr,
            timestamp_asserter_addr: self.l2_timestamp_asserter_addr,
            testnet_paymaster_addr: self.l2_testnet_paymaster_addr,
            multicall3: self.l2_multicall3,
            da_validator_addr: None,
        }
    }
}

async fn handle_rpc_response_with_fallback<T, F>(
    rpc_call: F,
    fallback: T,
    context: String,
) -> anyhow::Result<T>
where
    F: Future<Output = Result<T, ClientError>>,
    T: Clone,
{
    match rpc_call.await {
        Err(ClientError::Call(err))
            if [
                ErrorCode::MethodNotFound.code(),
                // This what `Web3Error::NotImplemented` gets
                // `casted` into in the `api` server.
                ErrorCode::InternalError.code(),
            ]
            .contains(&(err.code())) =>
        {
            Ok(fallback)
        }
        response => response.context(context),
    }
}
