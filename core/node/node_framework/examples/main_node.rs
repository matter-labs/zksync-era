//! An incomplete example of how node initialization looks like.
//! This example defines a `ResourceProvider` that works using the main node env config, and
//! initializes a single task with a health check server.

use zksync_config::{
    configs::chain::{MempoolConfig, NetworkConfig, OperationsManagerConfig, StateKeeperConfig},
    ContractsConfig, DBConfig, PostgresConfig,
};
use zksync_core::metadata_calculator::MetadataCalculatorConfig;
use zksync_env_config::FromEnv;
use zksync_node_framework::{
    implementations::layers::{
        healtcheck_server::HealthCheckLayer,
        metadata_calculator::MetadataCalculatorLayer,
        pools_layer::PoolsLayerBuilder,
        state_keeper::{
            main_node_batch_executor_builder::MainNodeBatchExecutorBuilderLayer,
            mempool_io::MempoolIOLayer, StateKeeperLayer,
        },
    },
    service::ZkStackService,
};

fn main() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    let _guard = vlog::ObservabilityBuilder::new()
        .with_log_format(log_format)
        .build();

    // Create the node with specified resource provider. We don't need to add any resources explicitly,
    // the task will request what they actually need. The benefit here is that we won't instantiate resources
    // that are not used, which would be complex otherwise, since the task set is often dynamic.
    let mut node = ZkStackService::new()?;

    // Add pools.
    let postgres_config = PostgresConfig::from_env()?;
    let pools_layer = PoolsLayerBuilder::empty(postgres_config)
        .with_master(true)
        .with_replica(true)
        .with_prover(true)
        .build();
    node.add_layer(pools_layer);

    // Add the metadata calculator task.
    let merkle_tree_env_config = DBConfig::from_env()?.merkle_tree;
    let operations_manager_env_config = OperationsManagerConfig::from_env()?;
    let metadata_calculator_config = MetadataCalculatorConfig::for_main_node(
        &merkle_tree_env_config,
        &operations_manager_env_config,
    );
    node.add_layer(MetadataCalculatorLayer(metadata_calculator_config));

    // Add the state keeper
    let mempool_io_layer = MempoolIOLayer::new(
        NetworkConfig::from_env()?,
        ContractsConfig::from_env()?,
        StateKeeperConfig::from_env()?,
        MempoolConfig::from_env()?,
    );
    let main_node_batch_executor_builder_layer = MainNodeBatchExecutorBuilderLayer::new(
        DBConfig::from_env()?,
        StateKeeperConfig::from_env()?,
    );
    let state_keeper_layer = StateKeeperLayer;
    node.add_layer(mempool_io_layer)
        .add_layer(main_node_batch_executor_builder_layer)
        .add_layer(state_keeper_layer);

    // Add the healthcheck server.
    let healthcheck_config = zksync_config::ApiConfig::from_env()?.healthcheck;
    node.add_layer(HealthCheckLayer(healthcheck_config));

    // Run the node until completion.
    node.run()?;

    Ok(())
}
