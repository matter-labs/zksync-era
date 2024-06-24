use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use zksync_config::configs::{api::MerkleTreeApiConfig, database::MerkleTreeMode};
use zksync_metadata_calculator::{
    LazyAsyncTreeReader, MerkleTreePruningTask, MetadataCalculator, MetadataCalculatorConfig,
};
use zksync_storage::RocksDB;

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
        web3_api::TreeApiClientResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Builder for a metadata calculator.
///
/// ## Effects
///
/// - Resolves `PoolResource<MasterPool>`.
/// - Resolves `PoolResource<ReplicaPool>`.
/// - Resolves `ObjectStoreResource` (optional).
/// - Adds `tree_health_check` to the `ResourceCollection<HealthCheckResource>`.
/// - Adds `metadata_calculator` to the node.
#[derive(Debug)]
pub struct MetadataCalculatorLayer {
    config: MetadataCalculatorConfig,
    tree_api_config: Option<MerkleTreeApiConfig>,
    pruning_config: Option<Duration>,
}

impl MetadataCalculatorLayer {
    pub fn new(config: MetadataCalculatorConfig) -> Self {
        Self {
            config,
            tree_api_config: None,
            pruning_config: None,
        }
    }

    pub fn with_tree_api_config(mut self, tree_api_config: MerkleTreeApiConfig) -> Self {
        self.tree_api_config = Some(tree_api_config);
        self
    }

    pub fn with_pruning_config(mut self, pruning_config: Duration) -> Self {
        self.pruning_config = Some(pruning_config);
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for MetadataCalculatorLayer {
    fn layer_name(&self) -> &'static str {
        "metadata_calculator_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context.get_resource::<PoolResource<MasterPool>>().await?;
        let main_pool = pool.get().await?;
        // The number of connections in a recovery pool is based on the mainnet recovery runs. It doesn't need
        // to be particularly accurate at this point, since the main node isn't expected to recover from a snapshot.
        let recovery_pool = context
            .get_resource::<PoolResource<ReplicaPool>>()
            .await?
            .get_custom(10)
            .await?;

        let object_store = match self.config.mode {
            MerkleTreeMode::Lightweight => None,
            MerkleTreeMode::Full => {
                let store = context.get_resource::<ObjectStoreResource>().await?;
                Some(store)
            }
        };

        let mut metadata_calculator = MetadataCalculator::new(
            self.config,
            object_store.map(|store_resource| store_resource.0),
            main_pool,
        )
        .await?
        .with_recovery_pool(recovery_pool);

        let AppHealthCheckResource(app_health) = context.get_resource_or_default().await;
        app_health
            .insert_custom_component(Arc::new(metadata_calculator.tree_health_check()))
            .map_err(WiringError::internal)?;

        if let Some(tree_api_config) = self.tree_api_config {
            let bind_addr = (Ipv4Addr::UNSPECIFIED, tree_api_config.port).into();
            let tree_reader = metadata_calculator.tree_reader();
            context.add_task(Box::new(TreeApiTask {
                bind_addr,
                tree_reader,
            }));
        }

        if let Some(pruning_removal_delay) = self.pruning_config {
            let pruning_task = Box::new(metadata_calculator.pruning_task(pruning_removal_delay));
            app_health
                .insert_component(pruning_task.health_check())
                .map_err(|err| WiringError::Internal(err.into()))?;
            context.add_task(pruning_task);
        }

        context.insert_resource(TreeApiClientResource(Arc::new(
            metadata_calculator.tree_reader(),
        )))?;

        let metadata_calculator_task = Box::new(MetadataCalculatorTask {
            metadata_calculator,
        });
        context.add_task(metadata_calculator_task);

        Ok(())
    }
}

#[derive(Debug)]
pub struct MetadataCalculatorTask {
    metadata_calculator: MetadataCalculator,
}

#[async_trait::async_trait]
impl Task for MetadataCalculatorTask {
    fn id(&self) -> TaskId {
        "metadata_calculator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let result = self.metadata_calculator.run(stop_receiver.0).await;

        // Wait for all the instances of RocksDB to be destroyed.
        tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
            .await
            .context("failed terminating RocksDB instances")?;
        result
    }
}

#[derive(Debug)]
pub struct TreeApiTask {
    bind_addr: SocketAddr,
    tree_reader: LazyAsyncTreeReader,
}

#[async_trait::async_trait]
impl Task for TreeApiTask {
    fn id(&self) -> TaskId {
        "tree_api".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        if let Some(reader) = self.tree_reader.wait().await {
            reader.run_api_server(self.bind_addr, stop_receiver.0).await
        } else {
            // Tree is dropped before initialized, e.g. because the node is getting shut down.
            // We don't want to treat this as an error since it could mask the real shutdown cause in logs etc.
            stop_receiver.0.changed().await?;
            Ok(())
        }
    }
}

#[async_trait::async_trait]
impl Task for MerkleTreePruningTask {
    fn id(&self) -> TaskId {
        "merkle_tree_pruning_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
