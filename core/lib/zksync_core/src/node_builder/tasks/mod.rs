use super::ZkSyncNode;

#[async_trait::async_trait]
trait ZkSyncTask: Send + Sync + 'static {
    type Config;

    fn new(node: &ZkSyncNode, config: impl Into<Self::Config>) -> Self;
    async fn run(self) -> anyhow::Result<()>;
}
