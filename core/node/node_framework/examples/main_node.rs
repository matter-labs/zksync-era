//! An incomplete example of how node initialization looks like.
//! This example defines a `ResourceProvider` that works using the main node env config, and
//! initializes a single task with a health check server.

use anyhow::Context;
use zksync_config::{
    configs::{
        chain::{
            CircuitBreakerConfig, MempoolConfig, NetworkConfig, OperationsManagerConfig,
            StateKeeperConfig,
        },
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        wallets::Wallets,
        DatabaseSecrets, FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig,
        L1Secrets, ObservabilityConfig, ProofDataHandlerConfig,
    },
    ApiConfig, ContractVerifierConfig, ContractsConfig, DBConfig, EthConfig, EthWatchConfig,
    GasAdjusterConfig, GenesisConfig, ObjectStoreConfig, PostgresConfig,
};
use zksync_env_config::FromEnv;
use zksync_metadata_calculator::MetadataCalculatorConfig;
use zksync_node_api_server::{
    tx_sender::{ApiContracts, TxSenderConfig},
    web3::{state::InternalApiConfig, Namespace},
};
use zksync_node_framework::{
    implementations::layers::{
        circuit_breaker_checker::CircuitBreakerCheckerLayer,
        commitment_generator::CommitmentGeneratorLayer,
        contract_verification_api::ContractVerificationApiLayer,
        eth_sender::{EthTxAggregatorLayer, EthTxManagerLayer},
        eth_watch::EthWatchLayer,
        healtcheck_server::HealthCheckLayer,
        house_keeper::HouseKeeperLayer,
        l1_gas::SequencerL1GasLayer,
        metadata_calculator::MetadataCalculatorLayer,
        object_store::ObjectStoreLayer,
        pk_signing_eth_client::PKSigningEthClientLayer,
        pools_layer::PoolsLayerBuilder,
        proof_data_handler::ProofDataHandlerLayer,
        query_eth_client::QueryEthClientLayer,
        sigint::SigintHandlerLayer,
        state_keeper::{
            main_batch_executor::MainBatchExecutorLayer, mempool_io::MempoolIOLayer,
            output_handler::OutputHandlerLayer, StateKeeperLayer,
        },
        web3_api::{
            caches::MempoolCacheLayer,
            server::{Web3ServerLayer, Web3ServerOptionalConfig},
            tree_api_client::TreeApiClientLayer,
            tx_sender::{PostgresStorageCachesConfig, TxSenderLayer},
            tx_sink::MasterPoolSinkLayer,
        },
    },
    service::{ZkStackService, ZkStackServiceBuilder, ZkStackServiceError},
};
use zksync_state::RocksdbStorageOptions;

struct MainNodeBuilder {
    node: ZkStackServiceBuilder,
}

impl MainNodeBuilder {
    fn new() -> Self {
        Self {
            node: ZkStackServiceBuilder::new(),
        }
    }

    fn add_sigint_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SigintHandlerLayer);
        Ok(self)
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        let config = PostgresConfig::from_env()?;
        let secrets = DatabaseSecrets::from_env()?;
        let pools_layer = PoolsLayerBuilder::empty(config, secrets)
            .with_master(true)
            .with_replica(true)
            .with_prover(true)
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_pk_signing_client_layer(mut self) -> anyhow::Result<Self> {
        let genesis = GenesisConfig::from_env()?;
        let eth_config = EthConfig::from_env()?;
        let wallets = Wallets::from_env()?;
        self.node.add_layer(PKSigningEthClientLayer::new(
            eth_config,
            ContractsConfig::from_env()?,
            genesis.l1_chain_id,
            wallets.eth_sender.context("Eth sender configs")?,
        ));
        Ok(self)
    }

    fn add_query_eth_client_layer(mut self) -> anyhow::Result<Self> {
        let genesis = GenesisConfig::from_env()?;
        let eth_config = L1Secrets::from_env()?;
        let query_eth_client_layer =
            QueryEthClientLayer::new(genesis.l1_chain_id, eth_config.l1_rpc_url);
        self.node.add_layer(query_eth_client_layer);
        Ok(self)
    }

    fn add_sequencer_l1_gas_layer(mut self) -> anyhow::Result<Self> {
        let gas_adjuster_config = GasAdjusterConfig::from_env()?;
        let state_keeper_config = StateKeeperConfig::from_env()?;
        let genesis_config = GenesisConfig::from_env()?;
        let eth_sender_config = EthConfig::from_env()?;
        let sequencer_l1_gas_layer = SequencerL1GasLayer::new(
            gas_adjuster_config,
            genesis_config,
            state_keeper_config,
            eth_sender_config
                .sender
                .context("eth_sender")?
                .pubdata_sending_mode,
        );
        self.node.add_layer(sequencer_l1_gas_layer);
        Ok(self)
    }

    fn add_object_store_layer(mut self) -> anyhow::Result<Self> {
        let object_store_config = ObjectStoreConfig::from_env()?;
        self.node
            .add_layer(ObjectStoreLayer::new(object_store_config));
        Ok(self)
    }

    fn add_metadata_calculator_layer(mut self) -> anyhow::Result<Self> {
        let merkle_tree_env_config = DBConfig::from_env()?.merkle_tree;
        let operations_manager_env_config = OperationsManagerConfig::from_env()?;
        let state_keeper_env_config = StateKeeperConfig::from_env()?;
        let metadata_calculator_config = MetadataCalculatorConfig::for_main_node(
            &merkle_tree_env_config,
            &operations_manager_env_config,
            &state_keeper_env_config,
        );
        self.node
            .add_layer(MetadataCalculatorLayer::new(metadata_calculator_config));
        Ok(self)
    }

    fn add_state_keeper_layer(mut self) -> anyhow::Result<Self> {
        let wallets = Wallets::from_env()?;
        let contracts_config = ContractsConfig::from_env()?;
        let sk_config = StateKeeperConfig::from_env()?;
        let persisence_layer = OutputHandlerLayer::new(
            contracts_config.l2_shared_bridge_addr.unwrap(),
            sk_config.l2_block_seal_queue_capacity,
        );
        let mempool_io_layer = MempoolIOLayer::new(
            NetworkConfig::from_env()?.zksync_network_id,
            sk_config,
            MempoolConfig::from_env()?,
            wallets.state_keeper.context("State keeper wallets")?,
        );
        let main_node_batch_executor_builder_layer =
            MainBatchExecutorLayer::new(StateKeeperConfig::from_env()?.save_call_traces, true);
        let db_config = DBConfig::from_env()?;

        let rocksdb_options = RocksdbStorageOptions {
            block_cache_capacity: db_config
                .experimental
                .state_keeper_db_block_cache_capacity(),
            max_open_files: db_config.experimental.state_keeper_db_max_open_files,
        };
        let state_keeper_layer =
            StateKeeperLayer::new(db_config.state_keeper_db_path, rocksdb_options);
        self.node
            .add_layer(persisence_layer)
            .add_layer(mempool_io_layer)
            .add_layer(main_node_batch_executor_builder_layer)
            .add_layer(state_keeper_layer);
        Ok(self)
    }

    fn add_eth_watch_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(EthWatchLayer::new(
            EthWatchConfig::from_env()?,
            ContractsConfig::from_env()?,
        ));
        Ok(self)
    }

    fn add_proof_data_handler_layer(mut self) -> anyhow::Result<Self> {
        let genesis_config = GenesisConfig::from_env()?;
        self.node.add_layer(ProofDataHandlerLayer::new(
            ProofDataHandlerConfig::from_env()?,
            genesis_config.l1_batch_commit_data_generator_mode,
        ));
        Ok(self)
    }

    fn add_healthcheck_layer(mut self) -> anyhow::Result<Self> {
        let healthcheck_config = ApiConfig::from_env()?.healthcheck;
        self.node.add_layer(HealthCheckLayer(healthcheck_config));
        Ok(self)
    }

    fn add_tx_sender_layer(mut self) -> anyhow::Result<Self> {
        let state_keeper_config = StateKeeperConfig::from_env()?;
        let rpc_config = ApiConfig::from_env()?.web3_json_rpc;
        let network_config = NetworkConfig::from_env()?;
        let postgres_storage_caches_config = PostgresStorageCachesConfig {
            factory_deps_cache_size: rpc_config.factory_deps_cache_size() as u64,
            initial_writes_cache_size: rpc_config.initial_writes_cache_size() as u64,
            latest_values_cache_size: rpc_config.latest_values_cache_size() as u64,
        };
        let wallets = Wallets::from_env()?;

        // On main node we always use master pool sink.
        self.node.add_layer(MasterPoolSinkLayer);
        self.node.add_layer(TxSenderLayer::new(
            TxSenderConfig::new(
                &state_keeper_config,
                &rpc_config,
                wallets
                    .state_keeper
                    .context("StateKeeper wallets")?
                    .fee_account
                    .address(),
                network_config.zksync_network_id,
            ),
            postgres_storage_caches_config,
            rpc_config.vm_concurrency_limit(),
            ApiContracts::load_from_disk_blocking(), // TODO (BFT-138): Allow to dynamically reload API contracts
        ));
        Ok(self)
    }

    fn add_api_caches_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = ApiConfig::from_env()?.web3_json_rpc;
        self.node.add_layer(MempoolCacheLayer::new(
            rpc_config.mempool_cache_size(),
            rpc_config.mempool_cache_update_interval(),
        ));
        Ok(self)
    }

    fn add_tree_api_client_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = ApiConfig::from_env()?.web3_json_rpc;
        self.node
            .add_layer(TreeApiClientLayer::http(rpc_config.tree_api_url));
        Ok(self)
    }

    fn add_http_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = ApiConfig::from_env()?.web3_json_rpc;
        let contracts_config = ContractsConfig::from_env()?;
        let state_keeper_config = StateKeeperConfig::from_env()?;
        let with_debug_namespace = state_keeper_config.save_call_traces;
        let genesis_config = GenesisConfig::from_env()?;

        let mut namespaces = Namespace::DEFAULT.to_vec();
        if with_debug_namespace {
            namespaces.push(Namespace::Debug)
        }
        namespaces.push(Namespace::Snapshots);

        let optional_config = Web3ServerOptionalConfig {
            namespaces: Some(namespaces),
            filters_limit: Some(rpc_config.filters_limit()),
            subscriptions_limit: Some(rpc_config.subscriptions_limit()),
            batch_request_size_limit: Some(rpc_config.max_batch_request_size()),
            response_body_size_limit: Some(rpc_config.max_response_body_size()),
            ..Default::default()
        };
        self.node.add_layer(Web3ServerLayer::http(
            rpc_config.http_port,
            InternalApiConfig::new(&rpc_config, &contracts_config, &genesis_config),
            optional_config,
        ));

        Ok(self)
    }

    fn add_ws_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = ApiConfig::from_env()?.web3_json_rpc;
        let contracts_config = ContractsConfig::from_env()?;
        let genesis_config = GenesisConfig::from_env()?;
        let state_keeper_config = StateKeeperConfig::from_env()?;
        let circuit_breaker_config = CircuitBreakerConfig::from_env()?;
        let with_debug_namespace = state_keeper_config.save_call_traces;

        let mut namespaces = Namespace::DEFAULT.to_vec();
        if with_debug_namespace {
            namespaces.push(Namespace::Debug)
        }
        namespaces.push(Namespace::Snapshots);

        let optional_config = Web3ServerOptionalConfig {
            namespaces: Some(namespaces),
            filters_limit: Some(rpc_config.filters_limit()),
            subscriptions_limit: Some(rpc_config.subscriptions_limit()),
            batch_request_size_limit: Some(rpc_config.max_batch_request_size()),
            response_body_size_limit: Some(rpc_config.max_response_body_size()),
            websocket_requests_per_minute_limit: Some(
                rpc_config.websocket_requests_per_minute_limit(),
            ),
            replication_lag_limit: circuit_breaker_config.replication_lag_limit(),
            with_extended_tracing: rpc_config.extended_api_tracing,
            ..Default::default()
        };
        self.node.add_layer(Web3ServerLayer::ws(
            rpc_config.ws_port,
            InternalApiConfig::new(&rpc_config, &contracts_config, &genesis_config),
            optional_config,
        ));

        Ok(self)
    }
    fn add_eth_sender_layer(mut self) -> anyhow::Result<Self> {
        let eth_sender_config = EthConfig::from_env()?;
        let contracts_config = ContractsConfig::from_env()?;
        let network_config = NetworkConfig::from_env()?;
        let genesis_config = GenesisConfig::from_env()?;

        self.node.add_layer(EthTxAggregatorLayer::new(
            eth_sender_config.clone(),
            contracts_config,
            network_config.zksync_network_id,
            genesis_config.l1_batch_commit_data_generator_mode,
        ));
        self.node
            .add_layer(EthTxManagerLayer::new(eth_sender_config));

        Ok(self)
    }

    fn add_house_keeper_layer(mut self) -> anyhow::Result<Self> {
        let house_keeper_config = HouseKeeperConfig::from_env()?;
        let fri_prover_config = FriProverConfig::from_env()?;
        let fri_witness_generator_config = FriWitnessGeneratorConfig::from_env()?;
        let fri_prover_group_config = FriProverGroupConfig::from_env()?;
        let fri_proof_compressor_config = FriProofCompressorConfig::from_env()?;

        self.node.add_layer(HouseKeeperLayer::new(
            house_keeper_config,
            fri_prover_config,
            fri_witness_generator_config,
            fri_prover_group_config,
            fri_proof_compressor_config,
        ));

        Ok(self)
    }

    fn add_commitment_generator_layer(mut self) -> anyhow::Result<Self> {
        let genesis = GenesisConfig::from_env()?;
        self.node.add_layer(CommitmentGeneratorLayer::new(
            genesis.l1_batch_commit_data_generator_mode,
        ));

        Ok(self)
    }

    fn add_circuit_breaker_checker_layer(mut self) -> anyhow::Result<Self> {
        let circuit_breaker_config = CircuitBreakerConfig::from_env()?;
        self.node
            .add_layer(CircuitBreakerCheckerLayer(circuit_breaker_config));

        Ok(self)
    }

    fn add_contract_verification_api_layer(mut self) -> anyhow::Result<Self> {
        let config = ContractVerifierConfig::from_env()?;
        self.node.add_layer(ContractVerificationApiLayer(config));
        Ok(self)
    }

    fn build(mut self) -> Result<ZkStackService, ZkStackServiceError> {
        self.node.build()
    }
}

fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: zksync_vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;
    let _guard = zksync_vlog::ObservabilityBuilder::new()
        .with_log_format(log_format)
        .build();

    MainNodeBuilder::new()
        .add_sigint_handler_layer()?
        .add_pools_layer()?
        .add_circuit_breaker_checker_layer()?
        .add_query_eth_client_layer()?
        .add_sequencer_l1_gas_layer()?
        .add_object_store_layer()?
        .add_metadata_calculator_layer()?
        .add_state_keeper_layer()?
        .add_eth_watch_layer()?
        .add_pk_signing_client_layer()?
        .add_eth_sender_layer()?
        .add_proof_data_handler_layer()?
        .add_healthcheck_layer()?
        .add_tx_sender_layer()?
        .add_tree_api_client_layer()?
        .add_api_caches_layer()?
        .add_http_web3_api_layer()?
        .add_ws_web3_api_layer()?
        .add_house_keeper_layer()?
        .add_commitment_generator_layer()?
        .add_contract_verification_api_layer()?
        .build()?
        .run()?;

    Ok(())
}
