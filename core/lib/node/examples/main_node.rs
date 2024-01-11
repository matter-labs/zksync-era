use zksync_config::PostgresConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_node::{resources::pools::PoolsResource, ResourceProvider, ZkSyncNode};

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
    let node = ZkSyncNode::new(MainNodeResourceProvider)?;

    node.run()?;

    Ok(())
}
