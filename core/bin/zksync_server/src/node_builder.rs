//! This module provides a "builder" for the main node,
//! as well as an interface to run the node with the specified components.

use std::time::Duration;

use anyhow::{bail, Context};
use tokio::runtime::Runtime;
use zksync_base_token_adjuster::node::{
    BaseTokenRatioPersisterLayer, BaseTokenRatioProviderLayer, ExternalPriceApiLayer,
};
use zksync_circuit_breaker::node::CircuitBreakerCheckerLayer;
use zksync_commitment_generator::node::{
    CommitmentGeneratorLayer, L1BatchCommitmentModeValidationLayer,
};
use zksync_config::{
    configs::{
        consensus::ConsensusConfig,
        contracts::{
            chain::L2Contracts, ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts,
        },
        da_client::DAClientConfig,
        secrets::DataAvailabilitySecrets,
        wallets::Wallets,
        GeneralConfig, Secrets,
    },
    GenesisConfig,
};
use zksync_contract_verification_server::node::ContractVerificationApiLayer;
use zksync_core_leftovers::Component;
use zksync_da_clients::node::{
    AvailWiringLayer, CelestiaWiringLayer, EigenWiringLayer, NoDAClientWiringLayer,
    ObjectStorageClientWiringLayer,
};
use zksync_da_dispatcher::node::DataAvailabilityDispatcherLayer;
use zksync_dal::node::{PoolsLayerBuilder, PostgresMetricsLayer};
use zksync_eth_client::{
    node::{BridgeAddressesUpdaterLayer, PKSigningEthClientLayer},
    web3_decl::node::{QueryEthClientLayer, SettlementLayerClientLayer},
};
use zksync_eth_sender::node::{EthTxAggregatorLayer, EthTxManagerLayer};
use zksync_eth_watch::node::EthWatchLayer;
use zksync_external_proof_integration_api::node::ExternalProofIntegrationApiLayer;
use zksync_gateway_migrator::node::{GatewayMigratorLayer, MainNodeConfig, SettlementLayerData};
use zksync_house_keeper::node::HouseKeeperLayer;
use zksync_logs_bloom_backfill::node::LogsBloomBackfillLayer;
use zksync_metadata_calculator::{
    node::{MetadataCalculatorLayer, TreeApiClientLayer},
    MetadataCalculatorConfig,
};
use zksync_node_api_server::{
    node::{
        DeploymentAllowListLayer, HealthCheckLayer, MasterPoolSinkLayer, MempoolCacheLayer,
        PostgresStorageCachesConfig, TxSenderLayer, Web3ServerLayer, Web3ServerOptionalConfig,
        WhitelistedMasterPoolSinkLayer,
    },
    tx_sender::TxSenderConfig,
    web3::{state::InternalApiConfigBase, Namespace},
};
use zksync_node_consensus::node::MainNodeConsensusLayer;
use zksync_node_fee_model::node::{GasAdjusterLayer, L1GasLayer};
use zksync_node_framework::service::{ZkStackService, ZkStackServiceBuilder};
use zksync_node_storage_init::node::{
    main_node_strategy::MainNodeInitStrategyLayer, NodeStorageInitializerLayer,
};
use zksync_object_store::node::ObjectStoreLayer;
use zksync_proof_data_handler::node::ProofDataHandlerLayer;
use zksync_state::RocksdbStorageOptions;
use zksync_state_keeper::node::{
    MainBatchExecutorLayer, MempoolIOLayer, OutputHandlerLayer, StateKeeperLayer,
};
use zksync_tee_proof_data_handler::node::TeeProofDataHandlerLayer;
use zksync_types::{
    commitment::{L1BatchCommitmentMode, PubdataType},
    pubdata_da::PubdataSendingMode,
    Address, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
};
use zksync_vlog::node::{PrometheusExporterLayer, SigintHandlerLayer};
use zksync_vm_runner::node::{
    BasicWitnessInputProducerLayer, ProtectiveReadsWriterLayer, VmPlaygroundLayer,
};

/// Macro that looks into a path to fetch an optional config,
/// and clones it into a variable.
macro_rules! try_load_config {
    ($path:expr) => {
        $path.as_ref().context(stringify!($path))?.clone()
    };
}

pub(crate) struct MainNodeBuilder {
    node: ZkStackServiceBuilder,
    configs: GeneralConfig,
    wallets: Wallets,
    genesis_config: GenesisConfig,
    consensus: Option<ConsensusConfig>,
    secrets: Secrets,
    l1_specific_contracts: L1SpecificContracts,
    // This field is a fallback for situation
    // if use pre v26 contracts and not all functions are available for loading contracts
    l1_sl_contracts: Option<SettlementLayerSpecificContracts>,
    l2_contracts: L2Contracts,
    multicall3: Option<Address>,
}

impl MainNodeBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime: Runtime,
        configs: GeneralConfig,
        wallets: Wallets,
        genesis_config: GenesisConfig,
        consensus: Option<ConsensusConfig>,
        secrets: Secrets,
        l1_specific_contracts: L1SpecificContracts,
        l2_contracts: L2Contracts,
        l1_sl_contracts: Option<SettlementLayerSpecificContracts>,
        multicall3: Option<Address>,
    ) -> Self {
        Self {
            node: ZkStackServiceBuilder::on_runtime(runtime),
            configs,
            wallets,
            genesis_config,
            consensus,
            secrets,
            l1_specific_contracts,
            l1_sl_contracts,
            l2_contracts,
            multicall3,
        }
    }

    pub fn get_pubdata_type(&self) -> anyhow::Result<PubdataType> {
        if self.genesis_config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Rollup
        {
            return Ok(PubdataType::Rollup);
        }

        match self.configs.da_client_config.clone() {
            None => Err(anyhow::anyhow!("No config for DA client")),
            Some(da_client_config) => Ok(match da_client_config {
                DAClientConfig::Avail(_) => PubdataType::Avail,
                DAClientConfig::Celestia(_) => PubdataType::Celestia,
                DAClientConfig::Eigen(_) => PubdataType::Eigen,
                DAClientConfig::ObjectStore(_) => PubdataType::ObjectStore,
                DAClientConfig::NoDA => PubdataType::NoDA,
            }),
        }
    }

    fn add_sigint_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(SigintHandlerLayer);
        Ok(self)
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        let config = self.configs.postgres_config.clone();
        let secrets = self.secrets.database.clone();
        let pools_layer = PoolsLayerBuilder::empty(config, secrets)
            .with_master(true)
            .with_replica(true)
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_prometheus_exporter_layer(mut self) -> anyhow::Result<Self> {
        let prom_config = &self.configs.prometheus_config;
        if let Some(prom_config) = prom_config.to_pull_config() {
            self.node.add_layer(PrometheusExporterLayer(prom_config));
        } else {
            tracing::info!("Prometheus listener port port is not configured; Prometheus exporter is not initialized");
        }
        Ok(self)
    }

    fn add_postgres_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(PostgresMetricsLayer);
        Ok(self)
    }

    fn add_pk_signing_client_layer(mut self) -> anyhow::Result<Self> {
        let gas_adjuster = try_load_config!(self.configs.eth).gas_adjuster;
        let operator = try_load_config!(self.wallets.operator);
        let blob_operator = self.wallets.blob_operator.clone();
        self.node.add_layer(PKSigningEthClientLayer::new(
            gas_adjuster,
            operator,
            blob_operator,
        ));
        Ok(self)
    }

    fn add_query_eth_client_layer(mut self) -> anyhow::Result<Self> {
        let genesis = self.genesis_config.clone();
        let eth_config = self.secrets.l1.clone();
        let query_eth_client_layer = QueryEthClientLayer::new(
            genesis.l1_chain_id,
            eth_config.l1_rpc_url.context("No L1 RPC URL")?,
        );
        self.node.add_layer(query_eth_client_layer);
        Ok(self)
    }

    fn add_settlement_layer_client_layer(mut self) -> anyhow::Result<Self> {
        let eth_config = self.secrets.l1.clone();
        let settlement_layer_client_layer = SettlementLayerClientLayer::new(
            eth_config.l1_rpc_url.context("No L1 RPC URL")?,
            eth_config.gateway_rpc_url,
        );
        self.node.add_layer(settlement_layer_client_layer);
        Ok(self)
    }

    fn add_gas_adjuster_layer(mut self) -> anyhow::Result<Self> {
        let gas_adjuster_config = try_load_config!(self.configs.eth).gas_adjuster;
        let gas_adjuster_layer =
            GasAdjusterLayer::new(gas_adjuster_config, self.genesis_config.clone());
        self.node.add_layer(gas_adjuster_layer);
        Ok(self)
    }

    fn add_l1_gas_layer(mut self) -> anyhow::Result<Self> {
        // Ensure the BaseTokenRatioProviderResource is inserted if the base token is not ETH.
        if self.l1_specific_contracts.base_token_address != SHARED_BRIDGE_ETHER_TOKEN_ADDRESS {
            let base_token_adjuster_config = self.configs.base_token_adjuster.clone();
            self.node
                .add_layer(BaseTokenRatioProviderLayer::new(base_token_adjuster_config));
        }
        let state_keeper_config = try_load_config!(self.configs.state_keeper_config);
        let l1_gas_layer = L1GasLayer::new(&state_keeper_config);
        self.node.add_layer(l1_gas_layer);
        Ok(self)
    }

    fn add_object_store_layer(mut self) -> anyhow::Result<Self> {
        let object_store_config = try_load_config!(self.configs.core_object_store);
        self.node
            .add_layer(ObjectStoreLayer::new(object_store_config));
        Ok(self)
    }

    fn add_l1_batch_commitment_mode_validation_layer(mut self) -> anyhow::Result<Self> {
        let layer = L1BatchCommitmentModeValidationLayer::new(
            self.genesis_config.l1_batch_commit_data_generator_mode,
        );
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_metadata_calculator_layer(mut self, with_tree_api: bool) -> anyhow::Result<Self> {
        let merkle_tree_env_config = self.configs.db_config.merkle_tree.clone();
        let operations_manager_env_config = self.configs.operations_manager_config.clone();
        let state_keeper_env_config = try_load_config!(self.configs.state_keeper_config);
        let metadata_calculator_config = MetadataCalculatorConfig::for_main_node(
            &merkle_tree_env_config,
            &operations_manager_env_config,
            &state_keeper_env_config,
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
        // Bytecode compression is currently mandatory for the transactions processed by the sequencer.
        const OPTIONAL_BYTECODE_COMPRESSION: bool = false;

        let sk_config = try_load_config!(self.configs.state_keeper_config);
        let persistence_layer = OutputHandlerLayer::new(sk_config.l2_block_seal_queue_capacity)
            .with_protective_reads_persistence_enabled(
                sk_config.protective_reads_persistence_enabled,
            );
        let mempool_io_layer = MempoolIOLayer::new(
            self.genesis_config.l2_chain_id,
            sk_config.clone(),
            self.configs.mempool_config.clone(),
            try_load_config!(self.wallets.fee_account),
            self.get_pubdata_type()?,
        );
        let db_config = self.configs.db_config.clone();
        let experimental_vm_config = self.configs.experimental_vm_config.clone();
        let main_node_batch_executor_builder_layer =
            MainBatchExecutorLayer::new(sk_config.save_call_traces, OPTIONAL_BYTECODE_COMPRESSION)
                .with_fast_vm_mode(experimental_vm_config.state_keeper_fast_vm_mode);

        let rocksdb_options = RocksdbStorageOptions {
            block_cache_capacity: db_config
                .experimental
                .state_keeper_db_block_cache_capacity_mb
                .0 as usize,
            max_open_files: db_config.experimental.state_keeper_db_max_open_files,
        };
        let state_keeper_layer =
            StateKeeperLayer::new(db_config.state_keeper_db_path, rocksdb_options);
        self.node
            .add_layer(persistence_layer)
            .add_layer(mempool_io_layer)
            .add_layer(main_node_batch_executor_builder_layer)
            .add_layer(state_keeper_layer);
        Ok(self)
    }

    fn add_eth_watch_layer(mut self) -> anyhow::Result<Self> {
        let eth_config = try_load_config!(self.configs.eth);
        self.node.add_layer(EthWatchLayer::new(
            eth_config.watcher,
            self.genesis_config.l2_chain_id,
        ));
        Ok(self)
    }

    fn add_settlement_mode_data(mut self) -> anyhow::Result<Self> {
        self.node
            .add_layer(SettlementLayerData::new(MainNodeConfig {
                l2_contracts: self.l2_contracts.clone(),
                l1_specific_contracts: self.l1_specific_contracts.clone(),
                l1_sl_specific_contracts: self.l1_sl_contracts.clone(),
                l2_chain_id: self.genesis_config.l2_chain_id,
                multicall3: self.multicall3,
                gateway_rpc_url: self.secrets.l1.gateway_rpc_url.clone(),
                eth_sender_config: try_load_config!(self.configs.eth)
                    .get_eth_sender_config_for_sender_layer_data_layer()
                    .clone(),
            }));
        Ok(self)
    }

    fn add_gateway_migrator_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(GatewayMigratorLayer {
            l2_chain_id: self.genesis_config.l2_chain_id,
            gateway_migrator_config: self.configs.gateway_migrator_config.clone(),
        });
        Ok(self)
    }

    fn add_proof_data_handler_layer(mut self) -> anyhow::Result<Self> {
        let gateway_config = try_load_config!(self.configs.prover_gateway);
        self.node.add_layer(ProofDataHandlerLayer::new(
            try_load_config!(self.configs.proof_data_handler_config),
            self.genesis_config.l1_batch_commit_data_generator_mode,
            self.genesis_config.l2_chain_id,
            gateway_config.api_mode,
        ));
        Ok(self)
    }

    fn add_tee_proof_data_handler_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(TeeProofDataHandlerLayer::new(
            try_load_config!(self.configs.tee_proof_data_handler_config),
            self.genesis_config.l1_batch_commit_data_generator_mode,
            self.genesis_config.l2_chain_id,
        ));
        Ok(self)
    }

    fn add_healthcheck_layer(mut self) -> anyhow::Result<Self> {
        let healthcheck_config = try_load_config!(self.configs.api_config).healthcheck;
        self.node.add_layer(HealthCheckLayer(healthcheck_config));
        Ok(self)
    }

    fn add_allow_list_task_layer(mut self) -> anyhow::Result<Self> {
        let allow_list = try_load_config!(self.configs.state_keeper_config).deployment_allowlist;

        if let Some(allow_list) = allow_list {
            self.node.add_layer(DeploymentAllowListLayer {
                deployment_allowlist: allow_list,
            });
        }
        Ok(self)
    }

    fn add_bridge_addresses_updater_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(BridgeAddressesUpdaterLayer {
            refresh_interval: Duration::from_secs(30),
        });
        Ok(self)
    }

    fn add_tx_sender_layer(mut self) -> anyhow::Result<Self> {
        let sk_config = try_load_config!(self.configs.state_keeper_config);
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        let deployment_allowlist = sk_config.deployment_allowlist.clone();

        let postgres_storage_caches_config = PostgresStorageCachesConfig {
            factory_deps_cache_size: rpc_config.factory_deps_cache_size_mb.0,
            initial_writes_cache_size: rpc_config.initial_writes_cache_size_mb.0,
            latest_values_cache_size: rpc_config.latest_values_cache_size_mb.0,
            latest_values_max_block_lag: rpc_config.latest_values_max_block_lag.get(),
        };
        let vm_config = self.configs.experimental_vm_config.clone();

        // On main node we always use master pool sink.
        if deployment_allowlist.is_some() {
            self.node.add_layer(WhitelistedMasterPoolSinkLayer);
        } else {
            self.node.add_layer(MasterPoolSinkLayer);
        }

        let layer = TxSenderLayer::new(
            postgres_storage_caches_config,
            rpc_config.vm_concurrency_limit,
            TxSenderConfig::new(
                &sk_config,
                &rpc_config,
                try_load_config!(self.wallets.fee_account).address(),
                self.genesis_config.l2_chain_id,
            ),
            self.configs.timestamp_asserter_config.clone(),
        );
        let layer = layer.with_vm_mode(vm_config.api_fast_vm_mode);
        self.node.add_layer(layer);
        Ok(self)
    }

    fn add_api_caches_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        self.node.add_layer(MempoolCacheLayer::new(
            rpc_config.mempool_cache_size,
            rpc_config.mempool_cache_update_interval,
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

        let mut namespaces = if let Some(namespaces) = &rpc_config.api_namespaces {
            namespaces
                .iter()
                .map(|a| a.parse())
                .collect::<Result<_, _>>()?
        } else {
            Namespace::DEFAULT.to_vec()
        };
        if with_debug_namespace {
            namespaces.push(Namespace::Debug)
        }
        namespaces.push(Namespace::Snapshots);

        let optional_config = Web3ServerOptionalConfig {
            namespaces: Some(namespaces),
            filters_limit: Some(rpc_config.filters_limit),
            subscriptions_limit: Some(rpc_config.subscriptions_limit),
            batch_request_size_limit: Some(rpc_config.max_batch_request_size),
            response_body_size_limit: Some(rpc_config.max_response_body_size()),
            with_extended_tracing: rpc_config.extended_api_tracing,
            ..Default::default()
        };
        let http_port = rpc_config.http_port;
        let internal_config_base = InternalApiConfigBase::new(&self.genesis_config, &rpc_config)
            .with_l1_to_l2_txs_paused(self.configs.mempool_config.l1_to_l2_txs_paused);

        self.node.add_layer(Web3ServerLayer::http(
            http_port,
            internal_config_base,
            optional_config,
        ));

        Ok(self)
    }

    fn add_ws_web3_api_layer(mut self) -> anyhow::Result<Self> {
        let rpc_config = try_load_config!(self.configs.api_config).web3_json_rpc;
        let state_keeper_config = try_load_config!(self.configs.state_keeper_config);
        let circuit_breaker_config = &self.configs.circuit_breaker_config;
        let with_debug_namespace = state_keeper_config.save_call_traces;

        let mut namespaces = if let Some(namespaces) = &rpc_config.api_namespaces {
            namespaces
                .iter()
                .map(|a| a.parse())
                .collect::<Result<_, _>>()?
        } else {
            Namespace::DEFAULT.to_vec()
        };
        if with_debug_namespace {
            namespaces.push(Namespace::Debug)
        }
        namespaces.push(Namespace::Snapshots);

        let optional_config = Web3ServerOptionalConfig {
            namespaces: Some(namespaces),
            filters_limit: Some(rpc_config.filters_limit),
            subscriptions_limit: Some(rpc_config.subscriptions_limit),
            batch_request_size_limit: Some(rpc_config.max_batch_request_size),
            response_body_size_limit: Some(rpc_config.max_response_body_size()),
            websocket_requests_per_minute_limit: Some(
                rpc_config.websocket_requests_per_minute_limit,
            ),
            replication_lag_limit: circuit_breaker_config.replication_lag_limit,
            with_extended_tracing: rpc_config.extended_api_tracing,
            ..Default::default()
        };
        let ws_port = rpc_config.ws_port;
        let internal_config_base = InternalApiConfigBase::new(&self.genesis_config, &rpc_config)
            .with_l1_to_l2_txs_paused(self.configs.mempool_config.l1_to_l2_txs_paused);

        self.node.add_layer(Web3ServerLayer::ws(
            ws_port,
            internal_config_base,
            optional_config,
        ));

        Ok(self)
    }

    fn add_eth_tx_manager_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(EthTxManagerLayer);

        Ok(self)
    }

    fn add_eth_tx_aggregator_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(EthTxAggregatorLayer::new(
            self.genesis_config.l2_chain_id,
            self.genesis_config.l1_batch_commit_data_generator_mode,
        ));

        Ok(self)
    }

    fn add_house_keeper_layer(mut self) -> anyhow::Result<Self> {
        let house_keeper_config = self.configs.house_keeper_config.clone();
        self.node
            .add_layer(HouseKeeperLayer::new(house_keeper_config));
        Ok(self)
    }

    fn add_commitment_generator_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(CommitmentGeneratorLayer::default());

        Ok(self)
    }

    fn add_circuit_breaker_checker_layer(mut self) -> anyhow::Result<Self> {
        let circuit_breaker_config = self.configs.circuit_breaker_config.clone();
        self.node
            .add_layer(CircuitBreakerCheckerLayer(circuit_breaker_config));

        Ok(self)
    }

    fn add_contract_verification_api_layer(mut self) -> anyhow::Result<Self> {
        let config = self.configs.contract_verifier.clone();
        self.node.add_layer(ContractVerificationApiLayer(config));
        Ok(self)
    }

    fn add_consensus_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(MainNodeConsensusLayer {
            config: self
                .consensus
                .clone()
                .context("Consensus config has to be provided")?,
            secrets: self.secrets.consensus.clone(),
        });

        Ok(self)
    }

    fn add_da_client_layer(mut self) -> anyhow::Result<Self> {
        let eth_sender_config = try_load_config!(self.configs.eth);
        // It's safe to use it temporary here. Preferably to move it to proper wiring layer
        let sender_config = eth_sender_config.get_eth_sender_config_for_sender_layer_data_layer();
        if sender_config.pubdata_sending_mode != PubdataSendingMode::Custom {
            tracing::warn!("DA dispatcher is enabled, but the pubdata sending mode is not `Custom`. DA client will not be started.");
            return Ok(self);
        }

        let da_client_config = self
            .configs
            .da_client_config
            .clone()
            .context("No config for DA client")?;

        if matches!(da_client_config, DAClientConfig::NoDA) {
            self.node.add_layer(NoDAClientWiringLayer);
            return Ok(self);
        }

        if let DAClientConfig::ObjectStore(config) = da_client_config {
            self.node
                .add_layer(ObjectStorageClientWiringLayer::new(config));
            return Ok(self);
        }

        let da_client_secrets = try_load_config!(self.secrets.data_availability);
        match (da_client_config, da_client_secrets) {
            (DAClientConfig::Avail(config), DataAvailabilitySecrets::Avail(secret)) => {
                self.node.add_layer(AvailWiringLayer::new(config, secret));
            }
            (DAClientConfig::Celestia(config), DataAvailabilitySecrets::Celestia(secret)) => {
                self.node
                    .add_layer(CelestiaWiringLayer::new(config, secret));
            }
            (DAClientConfig::Eigen(mut config), DataAvailabilitySecrets::Eigen(secret)) => {
                if config.eigenda_eth_rpc.is_none() {
                    let l1_secrets = &self.secrets.l1;
                    config.eigenda_eth_rpc = l1_secrets.l1_rpc_url.clone();
                }

                self.node.add_layer(EigenWiringLayer::new(config, secret));
            }
            _ => bail!("invalid pair of da_client and da_secrets"),
        }

        Ok(self)
    }

    fn add_da_dispatcher_layer(mut self) -> anyhow::Result<Self> {
        let eth_sender_config = try_load_config!(self.configs.eth);
        // It's safe to use it temporary here. Preferably to move it to proper wiring layer
        let sender_config = eth_sender_config.get_eth_sender_config_for_sender_layer_data_layer();
        if sender_config.pubdata_sending_mode != PubdataSendingMode::Custom {
            tracing::warn!("DA dispatcher is enabled, but the pubdata sending mode is not `Custom`. DA dispatcher will not be started.");
            return Ok(self);
        }

        let state_keeper_config = try_load_config!(self.configs.state_keeper_config);
        let da_config = try_load_config!(self.configs.da_dispatcher_config);
        self.node.add_layer(DataAvailabilityDispatcherLayer::new(
            state_keeper_config,
            da_config,
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

    fn add_external_api_client_layer(mut self) -> anyhow::Result<Self> {
        let config = self.configs.external_price_api_client_config.clone();
        self.node
            .add_layer(ExternalPriceApiLayer::try_from(config)?);
        Ok(self)
    }

    fn add_vm_runner_bwip_layer(mut self) -> anyhow::Result<Self> {
        let basic_witness_input_producer_config =
            try_load_config!(self.configs.basic_witness_input_producer_config);
        self.node.add_layer(BasicWitnessInputProducerLayer::new(
            basic_witness_input_producer_config,
            self.genesis_config.l2_chain_id,
        ));

        Ok(self)
    }

    fn add_vm_playground_layer(mut self) -> anyhow::Result<Self> {
        let vm_config = self.configs.experimental_vm_config.clone();
        self.node.add_layer(VmPlaygroundLayer::new(
            vm_config.playground,
            self.genesis_config.l2_chain_id,
        ));

        Ok(self)
    }

    fn add_base_token_ratio_persister_layer(mut self) -> anyhow::Result<Self> {
        let config = self.configs.base_token_adjuster.clone();
        let wallets = self.wallets.clone();
        let l1_chain_id = self.genesis_config.l1_chain_id;
        self.node.add_layer(BaseTokenRatioPersisterLayer::new(
            config,
            wallets,
            l1_chain_id,
        ));

        Ok(self)
    }

    fn add_external_proof_integration_api_layer(mut self) -> anyhow::Result<Self> {
        let config = try_load_config!(self.configs.external_proof_integration_api_config);
        let proof_data_handler_config = try_load_config!(self.configs.proof_data_handler_config);
        self.node.add_layer(ExternalProofIntegrationApiLayer::new(
            config,
            proof_data_handler_config,
            self.genesis_config.l1_batch_commit_data_generator_mode,
            self.genesis_config.l2_chain_id,
        ));

        Ok(self)
    }

    fn add_logs_bloom_backfill_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(LogsBloomBackfillLayer);
        Ok(self)
    }

    /// This layer will make sure that the database is initialized correctly,
    /// e.g. genesis will be performed if it's required.
    ///
    /// Depending on the `kind` provided, either a task or a precondition will be added.
    ///
    /// *Important*: the task should be added by at most one component, because
    /// it assumes unique control over the database. Multiple components adding this
    /// layer in a distributed mode may result in the database corruption.
    ///
    /// This task works in pair with precondition, which must be present in every component:
    /// the precondition will prevent node from starting until the database is initialized.
    fn add_storage_initialization_layer(mut self, kind: LayerKind) -> anyhow::Result<Self> {
        self.node.add_layer(MainNodeInitStrategyLayer {
            genesis: self.genesis_config.clone(),
        });
        let mut layer = NodeStorageInitializerLayer::new();
        if matches!(kind, LayerKind::Precondition) {
            layer = layer.as_precondition();
        }
        self.node.add_layer(layer);
        Ok(self)
    }

    /// Builds the node with the genesis initialization task only.
    pub fn only_genesis(mut self) -> anyhow::Result<ZkStackService> {
        self = self
            .add_pools_layer()?
            .add_query_eth_client_layer()?
            .add_settlement_mode_data()?
            .add_settlement_layer_client_layer()?
            .add_storage_initialization_layer(LayerKind::Task)?;

        Ok(self.node.build())
    }

    /// Builds the node with the specified components.
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
            .add_settlement_mode_data()?
            .add_settlement_layer_client_layer()?
            .add_gateway_migrator_layer()?
            .add_gas_adjuster_layer()?;

        // Add preconditions for all the components.
        self = self
            .add_l1_batch_commitment_mode_validation_layer()?
            .add_storage_initialization_layer(LayerKind::Precondition)?;

        // Sort the components, so that the components they may depend on each other are added in the correct order.
        components.sort_unstable_by_key(|component| match component {
            // API consumes the resources provided by other layers (multiple ones), so it has to come the last.
            Component::HttpApi | Component::WsApi => 1,
            // Default priority.
            _ => 0,
        });

        if components.contains(&Component::EthTxAggregator)
            | components.contains(&Component::EthTxManager)
        {
            self = self.add_pk_signing_client_layer()?;
        }

        // Add "component-specific" layers.
        // Note that the layers are added only once, so it's fine to add the same layer multiple times.
        for component in &components {
            match component {
                Component::StateKeeper => {
                    // State keeper is the core component of the sequencer,
                    // which is why we consider it to be responsible for the storage initialization.
                    self = self
                        .add_allow_list_task_layer()?
                        .add_l1_gas_layer()?
                        .add_storage_initialization_layer(LayerKind::Task)?
                        .add_state_keeper_layer()?
                        .add_logs_bloom_backfill_layer()?;
                }
                Component::HttpApi => {
                    self = self
                        .add_allow_list_task_layer()?
                        .add_bridge_addresses_updater_layer()?
                        .add_l1_gas_layer()?
                        .add_tx_sender_layer()?
                        .add_tree_api_client_layer()?
                        .add_api_caches_layer()?
                        .add_http_web3_api_layer()?;
                }
                Component::WsApi => {
                    self = self
                        .add_allow_list_task_layer()?
                        .add_bridge_addresses_updater_layer()?
                        .add_l1_gas_layer()?
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
                    self = self.add_eth_tx_aggregator_layer()?;
                }
                Component::EthTxManager => {
                    self = self.add_eth_tx_manager_layer()?;
                }
                Component::Housekeeper => {
                    self = self.add_house_keeper_layer()?.add_postgres_layer()?;
                }
                Component::ProofDataHandler => {
                    self = self.add_proof_data_handler_layer()?;
                }
                Component::TeeProofDataHandler => {
                    self = self.add_tee_proof_data_handler_layer()?;
                }
                Component::Consensus => {
                    self = self.add_consensus_layer()?;
                }
                Component::CommitmentGenerator => {
                    self = self.add_commitment_generator_layer()?;
                }
                Component::DADispatcher => {
                    self = self.add_da_client_layer()?.add_da_dispatcher_layer()?;
                }
                Component::VmRunnerProtectiveReads => {
                    self = self.add_vm_runner_protective_reads_layer()?;
                }
                Component::BaseTokenRatioPersister => {
                    self = self
                        .add_l1_gas_layer()?
                        .add_external_api_client_layer()?
                        .add_base_token_ratio_persister_layer()?;
                }
                Component::VmRunnerBwip => {
                    self = self.add_vm_runner_bwip_layer()?;
                }
                Component::VmPlayground => {
                    self = self.add_vm_playground_layer()?;
                }
                Component::ExternalProofIntegrationApi => {
                    self = self.add_external_proof_integration_api_layer()?;
                }
            }
        }
        Ok(self.node.build())
    }
}

/// Marker for layers that can add either a task or a precondition.
#[derive(Debug)]
enum LayerKind {
    Task,
    Precondition,
}
