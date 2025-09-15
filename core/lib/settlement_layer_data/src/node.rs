use anyhow::Context;
use zksync_basic_types::{
    commitment::{L1BatchCommitmentMode, L2DACommitmentScheme},
    settlement::{SettlementLayer, WorkingSettlementLayer},
    url::SensitiveUrl,
    Address, L2ChainId,
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
    CoreDal,
};
use zksync_eth_client::{
    contracts_loader::{
        get_server_notifier_addr, get_settlement_layer_from_l1, get_zk_chain_on_chain_params,
        load_settlement_layer_contracts,
    },
    node::SenderConfigResource,
    EthInterface,
};
use zksync_node_framework::{FromContext, IntoContext, WiringError, WiringLayer};
use zksync_shared_resources::{
    contracts::{
        L1ChainContractsResource, L1EcosystemContractsResource, L2ContractsResource,
        SettlementLayerContractsResource, ZkChainOnChainConfigResource,
    },
    DummyVerifierResource, L1BatchCommitmentModeResource, PubdataSendingModeResource,
};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_web3_decl::{
    client::{DynClient, L1, L2},
    namespaces::ZksNamespaceClient,
    node::{GatewayClientResource, SettlementLayerClient, SettlementModeResource},
};

use crate::{
    adjust_eth_sender_config, current_settlement_layer, get_db_settlement_mode, get_l2_client,
    remote_en_config::fetch_remote_en_config,
};

pub struct MainNodeConfig {
    pub l1_specific_contracts: L1SpecificContracts,
    // This contracts are required as a fallback
    pub l1_sl_specific_contracts: Option<SettlementLayerSpecificContracts>,
    pub l2_contracts: L2Contracts,
    pub l2_chain_id: L2ChainId,
    pub multicall3: Option<Address>,
    pub gateway_rpc_url: Option<SensitiveUrl>,
    pub eth_sender_config: SenderConfig,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub dummy_verifier: bool,
    pub config_l2_da_commitment_scheme: L2DACommitmentScheme,
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
    main_node_client: Option<Box<DynClient<L2>>>,
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
    dummy_verifier: DummyVerifierResource,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentModeResource,
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
        .unwrap_or(self.config.l1_sl_specific_contracts.clone().unwrap());

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

        let (mut sl_chain_contracts, mut zkchain_on_chain_config) = match &sl_client {
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

        if self.config.eth_sender_config.force_use_validator_timelock {
            sl_chain_contracts
                .ecosystem_contracts
                .validator_timelock_addr = self
                .config
                .l1_sl_specific_contracts
                .as_ref()
                .and_then(|sl_contracts| sl_contracts.ecosystem_contracts.validator_timelock_addr)
        }
        let eth_sender_config = adjust_eth_sender_config(
            self.config.eth_sender_config,
            final_settlement_mode.settlement_layer(),
        );

        if let Some(l2_da_commitment_scheme) = zkchain_on_chain_config.l2_da_commitment_scheme {
            if l2_da_commitment_scheme == L2DACommitmentScheme::None {
                tracing::warn!("L2 DA commitment scheme from on-chain config is None, falling back to the config value");
                zkchain_on_chain_config.l2_da_commitment_scheme =
                    Some(self.config.config_l2_da_commitment_scheme)
            } else if l2_da_commitment_scheme != self.config.config_l2_da_commitment_scheme {
                tracing::warn!("L2 DA commitment scheme from on-chain config ({l2_da_commitment_scheme:?}) does not match the config value ({:?}), using the on-chain value", self.config.config_l2_da_commitment_scheme);
            }
        }

        Ok(Output {
            initial_settlement_mode: SettlementModeResource::new(final_settlement_mode.clone()),
            contracts: SettlementLayerContractsResource(sl_chain_contracts),
            l1_ecosystem_contracts: L1EcosystemContractsResource(l1_specific_contracts),
            l1_contracts: L1ChainContractsResource(sl_l1_contracts),
            l2_contracts: L2ContractsResource(self.config.l2_contracts),
            pubdata_sending_mode: Some(PubdataSendingModeResource(
                eth_sender_config.pubdata_sending_mode,
            )),
            dummy_verifier: DummyVerifierResource(self.config.dummy_verifier),
            eth_sender_config: Some(SenderConfigResource(eth_sender_config)),
            sl_client,
            gateway_client: l2_eth_client.map(GatewayClientResource),
            zk_chain_on_chain_config: Some(ZkChainOnChainConfigResource(zkchain_on_chain_config)),
            l1_batch_commit_data_generator_mode: L1BatchCommitmentModeResource(
                self.config.l1_batch_commit_data_generator_mode,
            ),
        })
    }
}

#[derive(Debug)]
pub struct ENConfig {
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

        let mut connection = input
            .pool
            .get()
            .await?
            .connection()
            .await
            .context("failed getting pool connection")?;
        let initial_db_sl_mode = get_db_settlement_mode(&mut connection, chain_id).await?;
        let remote_config = fetch_remote_en_config(
            input
                .main_node_client
                .expect("Main node client is required for EN"),
        )
        .await;

        let remote_config = match remote_config {
            Ok(config) => {
                connection
                    .external_node_config_dal()
                    .save_config(&config)
                    .await
                    .context("failed to save remote config")?;
                config
            }
            Err(err) => {
                tracing::error!(
                    "Failed to fetch remote config: {} \n Using the cached config",
                    err
                );
                connection
                        .external_node_config_dal()
                        .get_en_remote_config()
                        .await
                        .context("failed to get remote config")?
                        .context("remote config is not set in the database, \
                        most likely it's your first run and main node should be available for this time")?
            }
        };

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
            dummy_verifier: DummyVerifierResource(remote_config.dummy_verifier),
            eth_sender_config: None,
            pubdata_sending_mode: None,
            zk_chain_on_chain_config: None,
            l1_batch_commit_data_generator_mode: L1BatchCommitmentModeResource(
                remote_config.l1_batch_commit_data_generator_mode,
            ),
        })
    }
}
