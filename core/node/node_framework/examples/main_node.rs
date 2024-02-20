//! An incomplete example of how node initialization looks like.
//! This example defines a `ResourceProvider` that works using the main node env config, and
//! initializes a single task with a health check server.

use anyhow::Context;
use zksync_config::{
    configs::{
        chain::{MempoolConfig, NetworkConfig, OperationsManagerConfig, StateKeeperConfig},
        ObservabilityConfig,
    },
    ApiConfig, ContractsConfig, DBConfig, ETHClientConfig, ETHWatchConfig, GasAdjusterConfig,
    ObjectStoreConfig, PostgresConfig,
};
use zksync_core::metadata_calculator::MetadataCalculatorConfig;
use zksync_env_config::FromEnv;
use zksync_node_framework::{
    implementations::layers::{
        eth_watch::EthWatchLayer,
        fee_input::SequencerFeeInputLayer,
        healtcheck_server::HealthCheckLayer,
        metadata_calculator::MetadataCalculatorLayer,
        object_store::ObjectStoreLayer,
        pools_layer::PoolsLayerBuilder,
        query_eth_client::QueryEthClientLayer,
        state_keeper::{
            main_batch_executor::MainBatchExecutorLayer, mempool_io::MempoolIOLayer,
            StateKeeperLayer,
        },
    },
    service::ZkStackService,
};

struct MainNodeBuilder {
    node: ZkStackService,
}

impl MainNodeBuilder {
    fn new() -> Self {
        Self {
            node: ZkStackService::new().expect("Failed to initialize the node"),
        }
    }

    fn add_pools_layer(mut self) -> anyhow::Result<Self> {
        let config = PostgresConfig::from_env()?;
        let pools_layer = PoolsLayerBuilder::empty(config)
            .with_master(true)
            .with_replica(true)
            .with_prover(true)
            .build();
        self.node.add_layer(pools_layer);
        Ok(self)
    }

    fn add_query_eth_client_layer(mut self) -> anyhow::Result<Self> {
        let eth_client_config = ETHClientConfig::from_env()?;
        let query_eth_client_layer = QueryEthClientLayer::new(eth_client_config.web3_url);
        self.node.add_layer(query_eth_client_layer);
        Ok(self)
    }

    fn add_fee_input_layer(mut self) -> anyhow::Result<Self> {
        let gas_adjuster_config = GasAdjusterConfig::from_env()?;
        let state_keeper_config = StateKeeperConfig::from_env()?;
        let fee_input_layer = SequencerFeeInputLayer::new(gas_adjuster_config, state_keeper_config);
        self.node.add_layer(fee_input_layer);
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
        let metadata_calculator_config = MetadataCalculatorConfig::for_main_node(
            &merkle_tree_env_config,
            &operations_manager_env_config,
        );
        self.node
            .add_layer(MetadataCalculatorLayer(metadata_calculator_config));
        Ok(self)
    }

    fn add_state_keeper_layer(mut self) -> anyhow::Result<Self> {
        let mempool_io_layer = MempoolIOLayer::new(
            NetworkConfig::from_env()?,
            ContractsConfig::from_env()?,
            StateKeeperConfig::from_env()?,
            MempoolConfig::from_env()?,
        );
        let main_node_batch_executor_builder_layer =
            MainBatchExecutorLayer::new(DBConfig::from_env()?, StateKeeperConfig::from_env()?);
        let state_keeper_layer = StateKeeperLayer;
        self.node
            .add_layer(mempool_io_layer)
            .add_layer(main_node_batch_executor_builder_layer)
            .add_layer(state_keeper_layer);
        Ok(self)
    }

    fn add_eth_watch_layer(mut self) -> anyhow::Result<Self> {
        self.node.add_layer(EthWatchLayer::new(
            ETHWatchConfig::from_env()?,
            ContractsConfig::from_env()?,
        ));
        Ok(self)
    }

    fn add_healthcheck_layer(mut self) -> anyhow::Result<Self> {
        let healthcheck_config = ApiConfig::from_env()?.healthcheck;
        self.node.add_layer(HealthCheckLayer(healthcheck_config));
        Ok(self)
    }

    fn build(self) -> ZkStackService {
        self.node
    }
}

fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;
    let _guard = vlog::ObservabilityBuilder::new()
        .with_log_format(log_format)
        .build();

    MainNodeBuilder::new()
        .add_pools_layer()?
        .add_query_eth_client_layer()?
        .add_fee_input_layer()?
        .add_object_store_layer()?
        .add_metadata_calculator_layer()?
        .add_state_keeper_layer()?
        .add_eth_watch_layer()?
        .add_healthcheck_layer()?
        .build()
        .run()?;

    Ok(())
}
