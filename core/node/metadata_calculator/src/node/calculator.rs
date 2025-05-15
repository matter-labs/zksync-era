use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use anyhow::Context;
use zksync_config::configs::{api::MerkleTreeApiConfig, database::MerkleTreeMode};
use zksync_dal::node::{MasterPool, PoolResource, ReplicaPool};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::ShutdownHook, FromContext, IntoContext, StopReceiver, Task, TaskId, WiringError,
    WiringLayer,
};
use zksync_object_store::ObjectStore;
use zksync_storage::RocksDB;

use super::tree_api_server::TreeApiTask;
use crate::{
    api_server::TreeApiClient, MerkleTreePruningTask, MetadataCalculator, MetadataCalculatorConfig,
    StaleKeysRepairTask,
};

/// Wiring layer for Metadata calculator and Tree API.
#[derive(Debug)]
pub struct MetadataCalculatorLayer {
    config: MetadataCalculatorConfig,
    tree_api_config: Option<MerkleTreeApiConfig>,
    pruning_config: Option<Duration>,
    stale_keys_repair_enabled: bool,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    replica_pool: PoolResource<ReplicaPool>,
    /// Only needed for `MerkleTreeMode::Full`
    object_store: Option<Arc<dyn ObjectStore>>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    metadata_calculator: MetadataCalculator,
    tree_api_client: Arc<dyn TreeApiClient>,
    /// Only provided if configuration is provided.
    #[context(task)]
    tree_api_task: Option<TreeApiTask>,
    /// Only provided if configuration is provided.
    #[context(task)]
    pruning_task: Option<MerkleTreePruningTask>,
    /// Only provided if enabled in the config.
    #[context(task)]
    stale_keys_repair_task: Option<StaleKeysRepairTask>,
    rocksdb_shutdown_hook: ShutdownHook,
}

impl MetadataCalculatorLayer {
    pub fn new(config: MetadataCalculatorConfig) -> Self {
        Self {
            config,
            tree_api_config: None,
            pruning_config: None,
            stale_keys_repair_enabled: false,
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

    pub fn with_stale_keys_repair(mut self) -> Self {
        self.stale_keys_repair_enabled = true;
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
        // The number of connections in a recovery pool is based on the Era mainnet recovery runs.
        let recovery_pool = input.replica_pool.get_custom(10).await?;
        let app_health = input.app_health;

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

        let mut metadata_calculator = MetadataCalculator::new(self.config, object_store, main_pool)
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
            .map(|pruning_removal_delay| {
                let pruning_task = metadata_calculator.pruning_task(pruning_removal_delay);
                app_health
                    .insert_component(pruning_task.health_check())
                    .map_err(WiringError::internal)?;
                Ok::<_, WiringError>(pruning_task)
            })
            .transpose()?;

        let stale_keys_repair_task = if self.stale_keys_repair_enabled {
            Some(metadata_calculator.stale_keys_repair_task())
        } else {
            None
        };

        let tree_api_client = Arc::new(metadata_calculator.tree_reader());

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
            stale_keys_repair_task,
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

#[async_trait::async_trait]
impl Task for StaleKeysRepairTask {
    fn id(&self) -> TaskId {
        "merkle_tree_stale_keys_repair_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
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
