use zksync_config::PostgresConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_node::{
    resources::pools::PoolsResource, tasks::metadata_calculator::MetadataCalculatorTask,
    IntoZkSyncTask, ResourceProvider, ZkSyncNode,
};

fn pools_resource() -> anyhow::Result<PoolsResource> {
    let config = PostgresConfig::from_env()?;
    let mut master_pool = ConnectionPool::builder(config.master_url()?, config.max_connections()?);
    master_pool.set_statement_timeout(config.statement_timeout());
    let mut replica_pool =
        ConnectionPool::builder(config.replica_url()?, config.max_connections()?);
    replica_pool.set_statement_timeout(config.statement_timeout());
    let mut prover_pool = ConnectionPool::builder(config.prover_url()?, config.max_connections()?);
    prover_pool.set_statement_timeout(config.statement_timeout());
    let pools = PoolsResource::default()
        .with_master_pool(master_pool)
        .with_replica_pool(replica_pool)
        .with_prover_pool(prover_pool);

    Ok(pools)
}

#[derive(Debug)]
struct MainNodeResourceProvider;

impl ResourceProvider for MainNodeResourceProvider {
    fn get_resource(&self, name: &str) -> Option<Box<dyn std::any::Any>> {
        match name {
            PoolsResource::RESOURCE_NAME => {
                let resource = pools_resource().expect("Failed to create pools resource");
                Some(Box::new(resource) as Box<dyn std::any::Any>)
            }
            _ => None,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let mut node = ZkSyncNode::new(MainNodeResourceProvider)?;

    let merkle_tree_env_config = zksync_config::DBConfig::from_env()?.merkle_tree;
    let operations_manager_env_config =
        zksync_config::configs::chain::OperationsManagerConfig::from_env()?;
    let metadata_calculator_config =
        zksync_core::metadata_calculator::MetadataCalculatorConfig::for_main_node(
            &merkle_tree_env_config,
            &operations_manager_env_config,
        );
    node.add_task("metadata_calculator", |node| {
        MetadataCalculatorTask::create(node, metadata_calculator_config)
    });

    node.run()?;

    Ok(())
}
