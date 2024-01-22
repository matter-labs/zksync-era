//! An incomplete example of how node initialization looks like.
//! This example defines a `ResourceProvider` that works using the main node env config, and
//! initializes a single task with a health check server.

use zksync_config::{configs::chain::OperationsManagerConfig, DBConfig, PostgresConfig};
use zksync_core::metadata_calculator::MetadataCalculatorConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_node::{
    implementations::{
        resource::pools::MasterPoolResource,
        task::{
            healtcheck_server::HealthCheckTaskBuilder,
            metadata_calculator::MetadataCalculatorTaskBuilder,
        },
    },
    node::ZkSyncNode,
    resource::{Resource, ResourceId, ResourceProvider, StoredResource},
};

/// Resource provider for the main node.
/// It defines which resources the tasks will receive. This particular provider is stateless, e.g. it always uses
/// the main node env config, and always knows which resources to provide.
/// The resource provider can be dynamic, however. For example, we can define a resource provider which may use
/// different config load scheme (e.g. env variables / protobuf / yaml / toml), and which resources to provide
/// (e.g. decide whether we need MempoolIO or ExternalIO depending on some config).
#[derive(Debug)]
struct MainNodeResourceProvider;

impl MainNodeResourceProvider {
    fn master_pool_resource() -> anyhow::Result<MasterPoolResource> {
        let config = PostgresConfig::from_env()?;
        let mut master_pool =
            ConnectionPool::builder(config.master_url()?, config.max_connections()?);
        master_pool.set_statement_timeout(config.statement_timeout());

        Ok(MasterPoolResource::new(master_pool))
    }
}

#[async_trait::async_trait]
impl ResourceProvider for MainNodeResourceProvider {
    async fn get_resource(&self, name: &ResourceId) -> Option<Box<dyn StoredResource>> {
        match name {
            name if name == &MasterPoolResource::resource_id() => {
                let resource =
                    Self::master_pool_resource().expect("Failed to create pools resource");
                Some(Box::new(resource))
            }
            _ => None,
        }
    }
}

fn main() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    let _guard = vlog::ObservabilityBuilder::new()
        .with_log_format(log_format)
        .build();

    // Create the node with specified resource provider. We don't need to add any resources explicitly,
    // the task will request what they actually need. The benefit here is that we won't instantiate resources
    // that are not used, which would be complex otherwise, since the task set is often dynamic.
    let mut node = ZkSyncNode::new(MainNodeResourceProvider)?;

    // Add the metadata calculator task.
    let merkle_tree_env_config = DBConfig::from_env()?.merkle_tree;
    let operations_manager_env_config = OperationsManagerConfig::from_env()?;
    let metadata_calculator_config = MetadataCalculatorConfig::for_main_node(
        &merkle_tree_env_config,
        &operations_manager_env_config,
    );
    node.add_task(MetadataCalculatorTaskBuilder(metadata_calculator_config));

    // Add the healthcheck server.
    let healthcheck_config = zksync_config::ApiConfig::from_env()?.healthcheck;
    node.add_task(HealthCheckTaskBuilder(healthcheck_config));

    // Run the node until completion.
    node.run()?;

    Ok(())
}
