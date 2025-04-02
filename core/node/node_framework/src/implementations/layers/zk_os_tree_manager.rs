use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use anyhow::Context as _;
use zksync_config::configs::api::MerkleTreeApiConfig;
use zksync_storage::RocksDB;
use zksync_zk_os_tree_manager::{LazyAsyncTreeReader, TreeManager, TreeManagerConfig};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
        web3_api::ZkOsTreeApiClientResource,
    },
    service::{ShutdownHook, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for Metadata calculator and Tree API.
#[derive(Debug)]
pub struct ZkOsTreeManagerLayer {
    config: TreeManagerConfig,
    tree_api_config: Option<MerkleTreeApiConfig>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub tree_manager: TreeManager,
    pub tree_api_client: ZkOsTreeApiClientResource,
    /// Only provided if configuration is provided.
    #[context(task)]
    pub tree_api_task: Option<TreeApiTask>,
    pub rocksdb_shutdown_hook: ShutdownHook,
}

impl ZkOsTreeManagerLayer {
    pub fn new(config: TreeManagerConfig) -> Self {
        Self {
            config,
            tree_api_config: None,
        }
    }

    pub fn with_tree_api_config(mut self, tree_api_config: MerkleTreeApiConfig) -> Self {
        self.tree_api_config = Some(tree_api_config);
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for ZkOsTreeManagerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tree_manager_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;
        let app_health = input.app_health.0;

        let tree_manager = TreeManager::new(self.config, main_pool);

        app_health
            .insert_custom_component(Arc::new(tree_manager.tree_health_check()))
            .map_err(WiringError::internal)?;

        let tree_api_task = self.tree_api_config.map(|tree_api_config| {
            let bind_addr = (Ipv4Addr::UNSPECIFIED, tree_api_config.port).into();
            let tree_reader = tree_manager.tree_reader();
            TreeApiTask {
                bind_addr,
                tree_reader,
            }
        });

        let tree_api_client = ZkOsTreeApiClientResource(Arc::new(tree_manager.tree_reader()));

        let rocksdb_shutdown_hook = ShutdownHook::new("rocksdb_terminaton", async {
            // Wait for all the instances of RocksDB to be destroyed.
            tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
                .await
                .context("failed terminating RocksDB instances")
        });

        Ok(Output {
            tree_manager,
            tree_api_client,
            tree_api_task,
            rocksdb_shutdown_hook,
        })
    }
}

#[async_trait::async_trait]
impl Task for TreeManager {
    fn id(&self) -> TaskId {
        "tree_manager".into()
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
        "zk_os_tree_api".into()
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
