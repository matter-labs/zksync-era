use std::future::Future;

use anyhow::Context;
use smart_config::{ConfigSchema, DescribeConfig, DeserializeConfig};
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{SharedStateKeeperConfig, TimestampAsserterConfig},
        consensus::ConsensusConfig,
        contracts::{
            chain::{ChainContracts, L2Contracts},
            ecosystem::{EcosystemCommonContracts, L1SpecificContracts},
            SettlementLayerSpecificContracts,
        },
        networks::{NetworksConfig, SharedL1ContractsConfig},
        CommitmentGeneratorConfig, ConsistencyCheckerConfig, DataAvailabilitySecrets, L1Secrets,
        ObservabilityConfig, PrometheusConfig, PruningConfig, Secrets, SnapshotRecoveryConfig,
    },
    ApiConfig, CapturedParams, ConfigRepository, DAClientConfig, DBConfig, ObjectStoreConfig,
    PostgresConfig,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;
#[cfg(test)]
use zksync_dal::{ConnectionPool, Core};
use zksync_node_api_server::{
    tx_sender::{TimestampAsserterParams, TxSenderConfig},
    web3::state::InternalApiConfigBase,
};
use zksync_types::{commitment::L1BatchCommitmentMode, Address, ETHEREUM_ADDRESS};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    jsonrpsee::{core::ClientError, types::error::ErrorCode},
    namespaces::{EnNamespaceClient, ZksNamespaceClient},
};

#[cfg(test)]
mod tests;

/// This part of the external node config is fetched directly from the main node.
#[derive(Debug)]
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
    pub base_token_addr: Address,
    pub l2_multicall3: Option<Address>,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub dummy_verifier: bool,
}

impl RemoteENConfig {
    pub async fn fetch(client: &DynClient<L2>) -> anyhow::Result<Self> {
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

/// Local configurations used by the node.
#[derive(Debug, DescribeConfig, DeserializeConfig)]
pub(crate) struct LocalConfig {
    #[config(nest)]
    pub api: ApiConfig,
    #[config(nest, deprecated = ".")]
    pub db: DBConfig,
    #[config(nest)]
    pub prometheus: PrometheusConfig,
    #[config(nest, alias = "database")]
    pub postgres: PostgresConfig,
    #[config(nest, deprecated = ".")]
    pub state_keeper: SharedStateKeeperConfig,
    #[config(nest, deprecated = "snapshots_recovery")]
    pub snapshot_recovery: SnapshotRecoveryConfig,
    #[config(nest)]
    pub pruning: PruningConfig,
    #[config(nest)]
    pub commitment_generator: CommitmentGeneratorConfig,
    #[config(nest)]
    pub timestamp_asserter: TimestampAsserterConfig,
    #[config(nest, deprecated = "da")]
    pub da_client: Option<DAClientConfig>,
    #[config(nest, alias = ".contracts.l1", deprecated = ".")]
    pub contracts: SharedL1ContractsConfig,
    #[config(nest, deprecated = ".")]
    pub networks: NetworksConfig,
    #[config(nest)]
    pub consistency_checker: ConsistencyCheckerConfig,
    #[config(flatten)]
    pub secrets: Secrets,
}

impl LocalConfig {
    /// Schema is chosen to be compatible both with file-based and env-based configs used for the node
    /// previously. This leads to deprecated aliases all around, which will hopefully be removed soon-ish.
    pub fn schema() -> anyhow::Result<ConfigSchema> {
        let mut schema = ConfigSchema::new(&Self::DESCRIPTION, "");
        // We don't get observability config from the schema directly.
        schema
            .insert(&ObservabilityConfig::DESCRIPTION, "observability")?
            .push_deprecated_alias("")?;
        // Consensus config is conditionally parsed.
        schema.insert(&ConsensusConfig::DESCRIPTION, "consensus")?;

        // Aliases for API configs.
        schema
            .single_mut(&Web3JsonRpcConfig::DESCRIPTION)?
            .push_alias("api")?
            .push_deprecated_alias("")?;
        schema
            .single_mut(&HealthCheckConfig::DESCRIPTION)?
            .push_deprecated_alias("healthcheck")?;
        schema
            .single_mut(&MerkleTreeApiConfig::DESCRIPTION)?
            .push_deprecated_alias("tree.api")?;

        schema
            .get_mut(
                &ObjectStoreConfig::DESCRIPTION,
                "snapshot_recovery.object_store",
            )
            .context("no object_store config for snapshot recovery")?
            .push_deprecated_alias("snapshots.object_store")?;
        schema
            .single_mut(&L1Secrets::DESCRIPTION)?
            .push_alias("networks")?
            .push_deprecated_alias("")?;
        schema
            .single_mut(&DataAvailabilitySecrets::DESCRIPTION)?
            .push_deprecated_alias("da_secrets")?;
        Ok(schema)
    }

    pub fn batch_transaction_updater_interval(&self) -> Duration {
        self.batch_transaction_updater_interval_sec
            .map_or_else(|| Duration::from_secs(5), |n| Duration::from_secs(n.get()))
    }

    #[cfg(test)]
    fn mock(temp_dir: &tempfile::TempDir, test_pool: &ConnectionPool<Core>) -> Self {
        use zksync_config::configs::{
            consensus::ConsensusSecrets, database::MerkleTreeConfig, secrets::PostgresSecrets,
            ContractVerifierSecrets, ExperimentalDBConfig,
        };

        let mut api = ApiConfig::for_tests();
        // Set all ports to 0 to assign free ports, so that they don't conflict for high-level tests.
        api.web3_json_rpc.http_port = 0;
        api.web3_json_rpc.ws_port = 0;
        api.merkle_tree.port = 0;
        api.healthcheck.port = 0;

        Self {
            api,
            db: DBConfig {
                state_keeper_db_path: temp_dir.path().join("state_keeper_cache"),
                merkle_tree: MerkleTreeConfig::for_tests(temp_dir.path().join("tree")),
                experimental: ExperimentalDBConfig::default(),
            },
            prometheus: PrometheusConfig::default(),
            postgres: PostgresConfig {
                max_connections: Some(test_pool.max_size()),
                ..PostgresConfig::default()
            },
            state_keeper: SharedStateKeeperConfig::default(),
            snapshot_recovery: SnapshotRecoveryConfig::default(),
            pruning: PruningConfig::default(),
            commitment_generator: CommitmentGeneratorConfig::default(),
            timestamp_asserter: TimestampAsserterConfig::default(),
            da_client: None,
            contracts: SharedL1ContractsConfig::default(),
            networks: NetworksConfig::for_tests(),
            consistency_checker: ConsistencyCheckerConfig::default(),
            secrets: Secrets {
                consensus: ConsensusSecrets::default(),
                postgres: PostgresSecrets {
                    server_url: Some(test_pool.database_url().clone()),
                    ..PostgresSecrets::default()
                },
                l1: L1Secrets {
                    l1_rpc_url: Some("http://localhost:8545/".parse().unwrap()), // Not used, but must be provided
                    ..L1Secrets::default()
                },
                data_availability: None,
                contract_verifier: ContractVerifierSecrets::default(),
            },
        }
    }
}

/// Generates all possible consensus secrets (from system entropy)
/// and prints them to stdout.
/// They should be copied over to the secrets.yaml/consensus_secrets.yaml file.
pub fn generate_consensus_secrets() {
    let validator_key = roles::validator::SecretKey::generate();
    let node_key = roles::node::SecretKey::generate();
    println!("# {}", validator_key.public().encode());
    println!("validator_key: {}", validator_key.encode());
    println!("# {}", node_key.public().encode());
    println!("node_key: {}", node_key.encode());
}

/// External Node Config contains all the configuration required for the EN operation.
/// It is split into three parts: required, optional and remote for easier navigation.
#[derive(Debug)]
pub(crate) struct ExternalNodeConfig<R = RemoteENConfig> {
    pub local: LocalConfig,
    pub consensus: Option<ConsensusConfig>,
    pub config_params: CapturedParams,
    pub remote: R,
}

impl ExternalNodeConfig<()> {
    /// Parses the local part of node configuration from the repo.
    ///
    /// **Important.** This method is blocking.
    pub fn new(mut repo: ConfigRepository<'_>, has_consensus: bool) -> anyhow::Result<Self> {
        repo.capture_parsed_params();
        Ok(Self {
            local: repo.parse()?,
            consensus: if has_consensus {
                repo.parse_opt()?
            } else {
                None
            },
            config_params: repo.into_captured_params(),
            remote: (),
        })
    }

    /// Fetches contracts addresses from the main node, completing the configuration.
    pub async fn fetch_remote(
        self,
        main_node_client: &DynClient<L2>,
    ) -> anyhow::Result<ExternalNodeConfig> {
        let remote = RemoteENConfig::fetch(main_node_client)
            .await
            .context("Unable to fetch required config values from the main node")?;

        let remote_diamond_proxy_addr = remote.l1_diamond_proxy_addr;
        if let Some(local_diamond_proxy_addr) = self.local.contracts.diamond_proxy_addr {
            anyhow::ensure!(
                local_diamond_proxy_addr == remote_diamond_proxy_addr,
                "L1 diamond proxy address {local_diamond_proxy_addr:?} specified in config doesn't match one returned \
                by main node ({remote_diamond_proxy_addr:?})"
            );
        } else {
            tracing::info!(
                "L1 diamond proxy address is not specified in config; will use address \
                returned by main node: {remote_diamond_proxy_addr:?}"
            );
        }

        Ok(ExternalNodeConfig {
            local: self.local,
            consensus: self.consensus,
            config_params: self.config_params,
            remote,
        })
    }
}

impl ExternalNodeConfig {
    #[cfg(test)]
    pub(crate) fn mock(temp_dir: &tempfile::TempDir, test_pool: &ConnectionPool<Core>) -> Self {
        Self {
            local: LocalConfig::mock(temp_dir, test_pool),
            consensus: None,
            config_params: CapturedParams::default(),
            remote: RemoteENConfig::mock(),
        }
    }

    /// Returns verified L1 diamond proxy address.
    /// If local configuration contains the address, it will be checked against the one returned by the main node.
    /// Otherwise, the remote value will be used. However, using remote value has trust implications for the main
    /// node so relying on it solely is not recommended.
    pub fn l1_diamond_proxy_address(&self) -> Address {
        self.local
            .contracts
            .diamond_proxy_addr
            .unwrap_or(self.remote.l1_diamond_proxy_addr)
    }
}

impl From<&ExternalNodeConfig> for InternalApiConfigBase {
    fn from(config: &ExternalNodeConfig) -> Self {
        let local = &config.local;
        let web3_rpc = &config.local.api.web3_json_rpc;
        Self {
            l1_chain_id: local.networks.l1_chain_id,
            l2_chain_id: local.networks.l2_chain_id,
            max_tx_size: web3_rpc.max_tx_size.0 as usize,
            estimate_gas_scale_factor: web3_rpc.estimate_gas_scale_factor,
            estimate_gas_acceptable_overestimation: web3_rpc.estimate_gas_acceptable_overestimation,
            estimate_gas_optimize_search: web3_rpc.estimate_gas_optimize_search,
            req_entities_limit: web3_rpc.req_entities_limit as usize,
            fee_history_limit: web3_rpc.fee_history_limit,
            filters_disabled: web3_rpc.filters_disabled,
            dummy_verifier: config.remote.dummy_verifier,
            l1_batch_commit_data_generator_mode: config.remote.l1_batch_commit_data_generator_mode,
            l1_to_l2_txs_paused: false,
        }
    }
}

impl From<&ExternalNodeConfig> for TxSenderConfig {
    fn from(config: &ExternalNodeConfig) -> Self {
        let local = &config.local;
        let web3_rpc = &local.api.web3_json_rpc;
        Self {
            // Fee account address does not matter for the EN operation, since
            // actual fee distribution is handled my the main node.
            fee_account_addr: "0xfee0000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            gas_price_scale_factor: web3_rpc.gas_price_scale_factor,
            max_nonce_ahead: web3_rpc.max_nonce_ahead,
            vm_execution_cache_misses_limit: web3_rpc.vm_execution_cache_misses_limit,
            // We set these values to the maximum since we don't know the actual values
            // and they will be enforced by the main node anyway.
            max_allowed_l2_tx_gas_limit: u64::MAX,
            validation_computational_gas_limit: u32::MAX,
            chain_id: local.networks.l2_chain_id,
            // Does not matter for EN.
            whitelisted_tokens_for_aa: Default::default(),
            timestamp_asserter_params: config.remote.l2_timestamp_asserter_addr.map(|address| {
                TimestampAsserterParams {
                    address,
                    min_time_till_end: local.timestamp_asserter.min_time_till_end,
                }
            }),
        }
    }
}

impl ExternalNodeConfig {
    pub fn l1_specific_contracts(&self) -> L1SpecificContracts {
        L1SpecificContracts {
            bytecodes_supplier_addr: self.remote.l1_bytecodes_supplier_addr,
            wrapped_base_token_store: self.remote.l1_wrapped_base_token_store,
            bridge_hub: self.remote.l1_bridgehub_proxy_addr,
            shared_bridge: self.remote.l1_shared_bridge_proxy_addr,
            erc_20_bridge: self.remote.l1_erc20_bridge_proxy_addr,
            base_token_address: self.remote.base_token_addr,
            server_notifier_addr: self.remote.l1_server_notifier_addr,
            // We don't need chain admin for external node
            chain_admin: None,
        }
    }

    pub fn l1_settelment_contracts(&self) -> SettlementLayerSpecificContracts {
        SettlementLayerSpecificContracts {
            ecosystem_contracts: EcosystemCommonContracts {
                bridgehub_proxy_addr: self.remote.l1_bridgehub_proxy_addr,
                state_transition_proxy_addr: self.remote.l1_state_transition_proxy_addr,
                // Multicall 3 is useless for external node
                multicall3: None,
                validator_timelock_addr: None,
            },
            chain_contracts_config: ChainContracts {
                diamond_proxy_addr: self.l1_diamond_proxy_address(),
            },
        }
    }

    pub fn l2_contracts(&self) -> L2Contracts {
        L2Contracts {
            erc20_default_bridge: self.remote.l2_erc20_bridge_addr,
            shared_bridge_addr: self.remote.l2_shared_bridge_addr,
            legacy_shared_bridge_addr: self.remote.l2_legacy_shared_bridge_addr,
            timestamp_asserter_addr: self.remote.l2_timestamp_asserter_addr,
            da_validator_addr: None,
            testnet_paymaster_addr: self.remote.l2_testnet_paymaster_addr,
            multicall3: self.remote.l2_multicall3,
        }
    }
}
