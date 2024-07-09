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
    service::{ShutdownHook, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for Metadata calculator and Tree API.
#[derive(Debug)]
pub struct MetadataCalculatorLayer {
    config: MetadataCalculatorConfig,
    tree_api_config: Option<MerkleTreeApiConfig>,
    pruning_config: Option<Duration>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub replica_pool: PoolResource<ReplicaPool>,
    /// Only needed for `MerkleTreeMode::Full`
    pub object_store: Option<ObjectStoreResource>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub metadata_calculator: MetadataCalculator,
    pub tree_api_client: TreeApiClientResource,
    /// Only provided if configuration is provided.
    #[context(task)]
    pub tree_api_task: Option<TreeApiTask>,
    /// Only provided if configuration is provided.
    #[context(task)]
    pub pruning_task: Option<MerkleTreePruningTask>,
    pub rocksdb_shutdown_hook: ShutdownHook,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "metadata_calculator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;
        // The number of connections in a recovery pool is based on the mainnet recovery runs. It doesn't need
        // to be particularly accurate at this point, since the main node isn't expected to recover from a snapshot.
        let recovery_pool = input.replica_pool.get_custom(10).await?;
        let app_health = input.app_health.0;

        let object_store = match self.config.mode {
            MerkleTreeMode::Lightweight => None,
            MerkleTreeMode::Full => {
                let store = input.object_store.ok_or_else(|| {
                    WiringError::Configuration(
                        "Object store is required for full Merkle tree mode".into(),
                    )
                })?;
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

        app_health
            .insert_custom_component(Arc::new(metadata_calculator.tree_health_check()))
            .map_err(WiringError::internal)?;

        let tree_api_task = self.tree_api_config.map(|tree_api_config| {
            let bind_addr = (Ipv4Addr::UNSPECIFIED, tree_api_config.port).into();
            let tree_reader = metadata_calculator.tree_reader();
            TreeApiTask {
                bind_addr,
                tree_reader,
            }
        });

        let pruning_task = self
            .pruning_config
            .map(
                |pruning_removal_delay| -> Result<MerkleTreePruningTask, WiringError> {
                    let pruning_task = metadata_calculator.pruning_task(pruning_removal_delay);
                    app_health
                        .insert_component(pruning_task.health_check())
                        .map_err(|err| WiringError::Internal(err.into()))?;
                    Ok(pruning_task)
                },
            )
            .transpose()?;

        let tree_api_client = TreeApiClientResource(Arc::new(metadata_calculator.tree_reader()));

        let rocksdb_shutdown_hook = ShutdownHook::new("rocksdb_terminaton", async {
            // Wait for all the instances of RocksDB to be destroyed.
            tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
                .await
                .context("failed terminating RocksDB instances")
        });

        Ok(Output {
            metadata_calculator,
            tree_api_client,
            tree_api_task,
            pruning_task,
            rocksdb_shutdown_hook,
        })
    }
}

#[async_trait::async_trait]
impl Task for MetadataCalculator {
    fn id(&self) -> TaskId {
        "metadata_calculator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
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
            tracing::warn!("Tree is dropped before initialized, not starting the tree API server");
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
