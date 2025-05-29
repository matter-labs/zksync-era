use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use zksync_config::configs::api::MerkleTreeApiConfig;
use zksync_node_framework::{
    task::TaskKind, IntoContext, StopReceiver, Task, TaskId, WiringError, WiringLayer,
};
use zksync_shared_resources::tree::TreeApiClient;

use crate::{LazyAsyncTreeReader, MerkleTreeReaderConfig, TreeReaderTask};

#[derive(Debug)]
pub struct TreeApiTask {
    pub(super) bind_addr: SocketAddr,
    pub(super) tree_reader: LazyAsyncTreeReader,
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

/// Mutually exclusive with [`MetadataCalculatorLayer`].
#[derive(Debug)]
pub struct TreeApiServerLayer {
    config: MerkleTreeReaderConfig,
    api_config: MerkleTreeApiConfig,
}

impl TreeApiServerLayer {
    pub fn new(config: MerkleTreeReaderConfig, api_config: MerkleTreeApiConfig) -> Self {
        Self { config, api_config }
    }
}

#[derive(Debug, IntoContext)]
pub struct Output {
    tree_api_client: Arc<dyn TreeApiClient>,
    #[context(task)]
    tree_reader_task: TreeReaderTask,
    #[context(task)]
    tree_api_task: TreeApiTask,
}

#[async_trait::async_trait]
impl WiringLayer for TreeApiServerLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tree_api_server"
    }

    async fn wire(self, (): Self::Input) -> Result<Self::Output, WiringError> {
        let tree_reader_task = TreeReaderTask::new(self.config);
        let bind_addr = (Ipv4Addr::UNSPECIFIED, self.api_config.port).into();
        let tree_api_task = TreeApiTask {
            bind_addr,
            tree_reader: tree_reader_task.tree_reader(),
        };
        Ok(Output {
            tree_api_client: Arc::new(tree_reader_task.tree_reader()),
            tree_api_task,
            tree_reader_task,
        })
    }
}

#[async_trait::async_trait]
impl Task for TreeReaderTask {
    fn kind(&self) -> TaskKind {
        TaskKind::OneshotTask
    }

    fn id(&self) -> TaskId {
        "merkle_tree_reader_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
