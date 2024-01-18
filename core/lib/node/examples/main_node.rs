//! An incomplete example of how node initialization looks like.
//! This example defines a `ResourceProvider` that works using the main node env config, and
//! initializes a single task with a health check server.

use zksync_config::PostgresConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_node::{
    healthcheck::IntoHealthCheckTask,
    implementations::{
        resource::pools::MasterPoolResource,
        task::{healtcheck_server::HealthCheckTask, metadata_calculator::MetadataCalculatorTask},
    },
    node::ZkSyncNode,
    resource::{Resource, ResourceProvider},
    task::IntoZkSyncTask,
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

impl ResourceProvider for MainNodeResourceProvider {
    fn get_resource(&self, name: &str) -> Option<Box<dyn std::any::Any>> {
        match name {
            MasterPoolResource::RESOURCE_NAME => {
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

    // Create the node with specified resource provider. We don't need to add any resourced explicitly,
    // the task will request what they actually need. The benefit here is that we won't instantiate resources
    // that are not used, which would be complex otherwise, since the task set is often dynamic.
    let mut node = ZkSyncNode::new(MainNodeResourceProvider)?;

    // Add the metadata calculator task.
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

    // Add the healthcheck server.
    let healthcheck_config = zksync_config::ApiConfig::from_env()?.healthcheck;
    node.with_healthcheck(move |node, healthchecks| {
        HealthCheckTask::create(node, healthchecks, healthcheck_config)
    });

    // Run the node until completion.
    node.run()?;

    Ok(())
}
