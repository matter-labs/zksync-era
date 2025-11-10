use anyhow::Context;
use smart_config::{ConfigSchema, DescribeConfig, DeserializeConfig};
use zksync_config::{
    configs::{
        api::{HealthCheckConfig, MerkleTreeApiConfig, Web3JsonRpcConfig},
        chain::{SharedStateKeeperConfig, TimestampAsserterConfig},
        consensus::ConsensusConfig,
        networks::{NetworksConfig, SharedL1ContractsConfig},
        CommitmentGeneratorConfig, ConsistencyCheckerConfig, DataAvailabilitySecrets, L1Secrets,
        NodeSyncConfig, ObservabilityConfig, PrometheusConfig, PruningConfig, Secrets,
        SnapshotRecoveryConfig,
    },
    ApiConfig, CapturedParams, ConfigRepository, DAClientConfig, DBConfig, ObjectStoreConfig,
    PostgresConfig,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;
#[cfg(test)]
use zksync_dal::{ConnectionPool, Core};
use zksync_node_api_server::{tx_sender::TxSenderConfig, web3::state::InternalApiConfigBase};

#[cfg(test)]
mod tests;

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
    #[config(nest)]
    pub node_sync: NodeSyncConfig,
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
        api.healthcheck.port = 0.into();

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
            node_sync: NodeSyncConfig::default(),
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
pub(crate) struct ExternalNodeConfig {
    pub local: LocalConfig,
    pub consensus: Option<ConsensusConfig>,
    pub config_params: CapturedParams,
}

impl ExternalNodeConfig {
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
        }
    }
}

impl From<&LocalConfig> for InternalApiConfigBase {
    fn from(config: &LocalConfig) -> Self {
        let state_keeper_config = &config.state_keeper;
        let web3_rpc = &config.api.web3_json_rpc;
        Self {
            l1_chain_id: config.networks.l1_chain_id,
            l2_chain_id: config.networks.l2_chain_id,
            max_tx_size: web3_rpc.max_tx_size.0 as usize,
            estimate_gas_scale_factor: web3_rpc.estimate_gas_scale_factor,
            estimate_gas_acceptable_overestimation: web3_rpc.estimate_gas_acceptable_overestimation,
            estimate_gas_optimize_search: web3_rpc.estimate_gas_optimize_search,
            req_entities_limit: web3_rpc.req_entities_limit as usize,
            fee_history_limit: web3_rpc.fee_history_limit,
            filters_disabled: web3_rpc.filters_disabled,
            l1_to_l2_txs_paused: false,
            eth_call_gas_cap: web3_rpc.eth_call_gas_cap,
            send_raw_tx_sync_max_timeout_ms: web3_rpc.send_raw_tx_sync_max_timeout_ms,
            send_raw_tx_sync_default_timeout_ms: web3_rpc.send_raw_tx_sync_default_timeout_ms,
            send_raw_tx_sync_poll_interval_ms: state_keeper_config
                .l2_block_commit_deadline
                .as_millis() as u64,
        }
    }
}

impl From<&LocalConfig> for TxSenderConfig {
    fn from(config: &LocalConfig) -> Self {
        let web3_rpc = &config.api.web3_json_rpc;
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
            chain_id: config.networks.l2_chain_id,
            // Does not matter for EN.
            whitelisted_tokens_for_aa: Default::default(),
            // It will be set in the wiring code.
            timestamp_asserter_params: None,
        }
    }
}
