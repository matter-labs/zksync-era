use zksync_config::PostgresConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_node::{
    healthcheck::IntoHealthCheckTask,
    node::ZkSyncNode,
    resource::{pools::MasterPoolResource, ResourceProvider},
    task::{
        healtcheck_server::HealthCheckTask, metadata_calculator::MetadataCalculatorTask,
        IntoZkSyncTask,
    },
};

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
                Some(Box::new(resource) as Box<dyn std::any::Any>)
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

    let healthcheck_config = zksync_config::ApiConfig::from_env()?.healthcheck;
    node.with_healthcheck(move |node, healthchecks| {
        HealthCheckTask::create(node, healthchecks, healthcheck_config)
    });

    node.run()?;

    Ok(())
}
