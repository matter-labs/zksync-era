//! This module provides a "builder" for the main node,
//! as well as an interface to run the node with the specified components.

use anyhow::Context;
use prometheus_exporter::PrometheusExporterConfig;
use zksync_config::{
    configs::{consensus::ConsensusConfig, wallets::Wallets, GeneralConfig, Secrets},
    ContractsConfig, GenesisConfig,
};
use zksync_core_leftovers::Component;
use zksync_metadata_calculator::MetadataCalculatorConfig;
use zksync_node_api_server::{
    tx_sender::{ApiContracts, TxSenderConfig},
    web3::{state::InternalApiConfig, Namespace},
};
use zksync_node_framework::{
    implementations::layers::{
        circuit_breaker_checker::CircuitBreakerCheckerLayer,
        commitment_generator::CommitmentGeneratorLayer,
        consensus::{ConsensusLayer, Mode as ConsensusMode},
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
        prometheus_exporter::PrometheusExporterLayer,
        proof_data_handler::ProofDataHandlerLayer,
        query_eth_client::QueryEthClientLayer,
        sigint::SigintHandlerLayer,
        state_keeper::{
            main_batch_executor::MainBatchExecutorLayer, mempool_io::MempoolIOLayer,
            StateKeeperLayer,
        },
        tee_verifier_input_producer::TeeVerifierInputProducerLayer,
        vm_runner::protective_reads::ProtectiveReadsWriterLayer,
        web3_api::{
            caches::MempoolCacheLayer,
            server::{Web3ServerLayer, Web3ServerOptionalConfig},
            tree_api_client::TreeApiClientLayer,
            tx_sender::{PostgresStorageCachesConfig, TxSenderLayer},
            tx_sink::TxSinkLayer,
        },
    },
    service::{ZkStackService, ZkStackServiceBuilder},
};

/// Macro that looks into a path to fetch an optional config,
/// and clones it into a variable.
macro_rules! try_load_config {
    ($path:expr) => {
        $path.as_ref().context(stringify!($path))?.clone()
    };
}

pub struct MainNodeBuilder {
    node: ZkStackServiceBuilder,
    configs: GeneralConfig,
    wallets: Wallets,
    genesis_config: GenesisConfig,
    contracts_config: ContractsConfig,
    secrets: Secrets,
    consensus_config: Option<ConsensusConfig>,
}

impl MainNodeBuilder {
    pub fn new(
        configs: GeneralConfig,
        wallets: Wallets,
        genesis_config: GenesisConfig,
        contracts_config: ContractsConfig,
        secrets: Secrets,
        consensus_config: Option<ConsensusConfig>,
    ) -> Self {
        Self {
            node: ZkStackServiceBuilder::new(),
            configs,
            wallets,
            genesis_config,
            contracts_config,
            secrets,
            consensus_config,
        }
    }

    fn add_sigint_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SigintHandlerLayer);
        Ok(self)
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        let config = try_load_config!(self.configs.postgres_config);
        let secrets = try_load_config!(self.secrets.database);
        let pools_layer = PoolsLayerBuilder::empty(config, secrets)
            .with_master(true)
            .with_replica(true)
            .with_prover(true) // Used by house keeper.
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_prometheus_exporter_layer(mut self) -> anyhow::Result<Self> {
        let prom_config = try_load_config!(self.configs.prometheus_config);
        let prom_config = PrometheusExporterConfig::pull(prom_config.listener_port);
        self.node.add_layer(PrometheusExporterLayer(prom_config));
        Ok(self)
    }

    fn add_pk_signing_client_layer(mut self) -> anyhow::Result<Self> {
        let eth_config = try_load_config!(self.configs.eth);
        let wallets = try_load_config!(self.wallets.eth_sender);
        self.node.add_layer(PKSigningEthClientLayer::new(
            eth_config,
            self.contracts_config.clone(),
            self.genesis_config.l1_chain_id,
            wallets,
        ));
        Ok(self)
    }

    fn add_query_eth_client_layer(mut self) -> anyhow::Result<Self> {
        let genesis = self.genesis_config.clone();
        let eth_config = try_load_config!(self.secrets.l1);
        let query_eth_client_layer =
            QueryEthClientLayer::new(genesis.l1_chain_id, eth_config.l1_rpc_url);
        self.node.add_layer(query_eth_client_layer);
        Ok(self)
    }

    fn add_sequencer_l1_gas_layer(mut self) -> anyhow::Result<Self> {
        let gas_adjuster_config = try_load_config!(self.configs.eth)
            .gas_adjuster
            .context("Gas adjuster")?;
        let state_keeper_config = try_load_config!(self.configs.state_keeper_config);
        let eth_sender_config = try_load_config!(self.configs.eth);
        let sequencer_l1_gas_layer = SequencerL1GasLayer::new(
            gas_adjuster_config,
            self.genesis_config.clone(),
            state_keeper_config,
            try_load_config!(eth_sender_config.sender).pubdata_sending_mode,
        );
        self.node.add_layer(sequencer_l1_gas_layer);
        Ok(self)
    }

    fn add_object_store_layer(mut self) -> anyhow::Result<Self> {
        let object_store_config = try_load_config!(self.configs.core_object_store);
        self.node
            .add_layer(ObjectStoreLayer::new(object_store_config));
        Ok(self)
    }

    fn add_metadata_calculator_layer(mut self, with_tree_api: bool) -> anyhow::Result<Self> {
        let merkle_tree_env_config = try_load_config!(self.configs.db_config).merkle_tree;
        let operations_manager_env_config =
            try_load_config!(self.configs.operations_manager_config);
        let metadata_calculator_config = MetadataCalculatorConfig::for_main_node(
            &merkle_tree_env_config,
            &operations_manager_env_config,
        );
        let mut layer = MetadataCalculatorLayer::new(metadata_calculator_config);
        if with_tree_api {
            let merkle_tree_api_config = try_load_config!(self.configs.api_config).merkle_tree;
            layer = layer.with_tree_api_config(merkle_tree_api_config);
        }
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_state_keeper_layer(mut self) -> anyhow::Result<Self> {
        let wallets = self.wallets.clone();
        let sk_config = try_load_config!(self.configs.state_keeper_config);
        let mempool_io_layer = MempoolIOLayer::new(
            self.genesis_config.l2_chain_id,
            self.contracts_config.clone(),
            sk_config.clone(),
            try_load_config!(self.configs.mempool_config),
            try_load_config!(wallets.state_keeper),
        );
        let db_config = try_load_config!(self.configs.db_config);
        let main_node_batch_executor_builder_layer = MainBatchExecutorLayer::new(sk_config);
        let state_keeper_layer = StateKeeperLayer::new(db_config);
        self.node
            .add_layer(mempool_io_layer)
            .add_layer(main_node_batch_executor_builder_layer)
            .add_layer(state_keeper_layer);
        Ok(self)
    }

    fn add_eth_watch_layer(mut self) -> anyhow::Result<Self> {
        let eth_config = try_load_config!(self.configs.eth);
        self.node.add_layer(EthWatchLayer::new(
            try_load_config!(eth_config.watcher),
            self.contracts_config.clone(),
        ));
        Ok(self)
    }

    fn add_proof_data_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(ProofDataHandlerLayer::new(
            try_load_config!(self.configs.proof_data_handler_config),
            self.genesis_config.l1_batch_commit_data_generator_mode,
        ));
        Ok(self)
    }

    fn add_healthcheck_layer(mut self) -> anyhow::Result<Self> {
        let healthcheck_config = try_load_config!(self.configs.api_config).healthcheck;
        self.node.add_layer(HealthCheckLayer(healthcheck_config));
        Ok(self)
    }

    fn add_tx_sender_layer(mut self) -> anyhow::Result<Self> {
        let sk_config = try_load_config!(self.configs.state_keeper_config);
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        let postgres_storage_caches_config = PostgresStorageCachesConfig {
            factory_deps_cache_size: rpc_config.factory_deps_cache_size() as u64,
            initial_writes_cache_size: rpc_config.initial_writes_cache_size() as u64,
            latest_values_cache_size: rpc_config.latest_values_cache_size() as u64,
        };

        // On main node we always use master pool sink.
        self.node.add_layer(TxSinkLayer::MasterPoolSink);
        self.node.add_layer(TxSenderLayer::new(
            TxSenderConfig::new(
                &sk_config,
                &rpc_config,
                try_load_config!(self.wallets.state_keeper)
                    .fee_account
                    .address(),
                self.genesis_config.l2_chain_id,
            ),
            postgres_storage_caches_config,
            rpc_config.vm_concurrency_limit(),
            ApiContracts::load_from_disk_blocking(), // TODO (BFT-138): Allow to dynamically reload API contracts
        ));
        Ok(self)
    }

    fn add_api_caches_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        self.node.add_layer(MempoolCacheLayer::new(
            rpc_config.mempool_cache_size(),
            rpc_config.mempool_cache_update_interval(),
        ));
        Ok(self)
    }

    fn add_tree_api_client_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        self.node
            .add_layer(TreeApiClientLayer::http(rpc_config.tree_api_url));
        Ok(self)
    }

    fn add_http_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        let state_keeper_config = try_load_config!(self.configs.state_keeper_config);
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
            ..Default::default()
        };
        self.node.add_layer(Web3ServerLayer::http(
            rpc_config.http_port,
            InternalApiConfig::new(&rpc_config, &self.contracts_config, &self.genesis_config),
            optional_config,
        ));

        Ok(self)
    }

    fn add_ws_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        let state_keeper_config = try_load_config!(self.configs.state_keeper_config);
        let circuit_breaker_config = try_load_config!(self.configs.circuit_breaker_config);
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
        };
        self.node.add_layer(Web3ServerLayer::ws(
            rpc_config.ws_port,
            InternalApiConfig::new(&rpc_config, &self.contracts_config, &self.genesis_config),
            optional_config,
        ));

        Ok(self)
    }

    fn add_eth_tx_manager_layer(mut self) -> anyhow::Result<Self> {
        let eth_sender_config = try_load_config!(self.configs.eth);

        self.node
            .add_layer(EthTxManagerLayer::new(eth_sender_config));

        Ok(self)
    }

    fn add_eth_tx_aggregator_layer(mut self) -> anyhow::Result<Self> {
        let eth_sender_config = try_load_config!(self.configs.eth);

        self.node.add_layer(EthTxAggregatorLayer::new(
            eth_sender_config,
            self.contracts_config.clone(),
            self.genesis_config.l2_chain_id,
            self.genesis_config.l1_batch_commit_data_generator_mode,
        ));

        Ok(self)
    }

    fn add_house_keeper_layer(mut self) -> anyhow::Result<Self> {
        let house_keeper_config = try_load_config!(self.configs.house_keeper_config);
        let fri_prover_config = try_load_config!(self.configs.prover_config);
        let fri_witness_generator_config = try_load_config!(self.configs.witness_generator);
        let fri_prover_group_config = try_load_config!(self.configs.prover_group_config);
        let fri_proof_compressor_config = try_load_config!(self.configs.proof_compressor_config);

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
        self.node.add_layer(CommitmentGeneratorLayer::new(
            self.genesis_config.l1_batch_commit_data_generator_mode,
        ));

        Ok(self)
    }

    fn add_circuit_breaker_checker_layer(mut self) -> anyhow::Result<Self> {
        let circuit_breaker_config = try_load_config!(self.configs.circuit_breaker_config);
        self.node
            .add_layer(CircuitBreakerCheckerLayer(circuit_breaker_config));

        Ok(self)
    }

    fn add_contract_verification_api_layer(mut self) -> anyhow::Result<Self> {
        let config = try_load_config!(self.configs.contract_verifier);
        self.node.add_layer(ContractVerificationApiLayer(config));
        Ok(self)
    }

    fn add_consensus_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(ConsensusLayer {
            mode: ConsensusMode::Main,
            config: self.consensus_config.clone(),
            secrets: self.secrets.consensus.clone(),
        });

        Ok(self)
    }

    fn add_tee_verifier_input_producer_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(TeeVerifierInputProducerLayer::new(
            self.genesis_config.l2_chain_id,
        ));

        Ok(self)
    }

    fn add_vm_runner_protective_reads_layer(mut self) -> anyhow::Result<Self> {
        let protective_reads_writer_config =
            try_load_config!(self.configs.protective_reads_writer_config);
        self.node.add_layer(ProtectiveReadsWriterLayer::new(
            protective_reads_writer_config,
            self.genesis_config.l2_chain_id,
        ));

        Ok(self)
    }

    pub fn build(mut self, mut components: Vec<Component>) -> anyhow::Result<ZkStackService> {
        // Add "base" layers (resources and helper tasks).
        self = self
            .add_sigint_handler_layer()?
            .add_pools_layer()?
            .add_object_store_layer()?
            .add_circuit_breaker_checker_layer()?
            .add_healthcheck_layer()?
            .add_prometheus_exporter_layer()?
            .add_query_eth_client_layer()?
            .add_sequencer_l1_gas_layer()?;

        // Sort the components, so that the components they may depend on each other are added in the correct order.
        components.sort_unstable_by_key(|component| match component {
            // API consumes the resources provided by other layers (multiple ones), so it has to come the last.
            Component::HttpApi | Component::WsApi => 1,
            // Default priority.
            _ => 0,
        });

        // Add "component-specific" layers.
        // Note that the layers are added only once, so it's fine to add the same layer multiple times.
        for component in &components {
            match component {
                Component::HttpApi => {
                    self = self
                        .add_tx_sender_layer()?
                        .add_tree_api_client_layer()?
                        .add_api_caches_layer()?
                        .add_http_web3_api_layer()?;
                }
                Component::WsApi => {
                    self = self
                        .add_tx_sender_layer()?
                        .add_tree_api_client_layer()?
                        .add_api_caches_layer()?
                        .add_ws_web3_api_layer()?;
                }
                Component::ContractVerificationApi => {
                    self = self.add_contract_verification_api_layer()?;
                }
                Component::Tree => {
                    let with_tree_api = components.contains(&Component::TreeApi);
                    self = self.add_metadata_calculator_layer(with_tree_api)?;
                }
                Component::TreeApi => {
                    anyhow::ensure!(
                        components.contains(&Component::Tree),
                        "Merkle tree API cannot be started without a tree component"
                    );
                    // Do nothing, will be handled by the `Tree` component.
                }
                Component::EthWatcher => {
                    self = self.add_eth_watch_layer()?;
                }
                Component::EthTxAggregator => {
                    self = self
                        .add_pk_signing_client_layer()?
                        .add_eth_tx_aggregator_layer()?;
                }
                Component::EthTxManager => {
                    self = self.add_eth_tx_manager_layer()?;
                }
                Component::StateKeeper => {
                    self = self.add_state_keeper_layer()?;
                }
                Component::TeeVerifierInputProducer => {
                    self = self.add_tee_verifier_input_producer_layer()?;
                }
                Component::Housekeeper => {
                    self = self.add_house_keeper_layer()?;
                }
                Component::ProofDataHandler => {
                    self = self.add_proof_data_handler_layer()?;
                }
                Component::Consensus => {
                    self = self.add_consensus_layer()?;
                }
                Component::CommitmentGenerator => {
                    self = self.add_commitment_generator_layer()?;
                }
                Component::VmRunnerProtectiveReads => {
                    self = self.add_vm_runner_protective_reads_layer()?;
                }
            }
        }
        Ok(self.node.build()?)
    }
}
