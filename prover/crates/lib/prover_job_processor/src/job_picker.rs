use async_trait::async_trait;

use crate::Executor;

#[async_trait]
pub trait JobPicker: Send + Sync {
    type ExecutorType: Executor;
    type Metadata;

    async fn pick_job(
        &self,
    ) -> anyhow::Result<Option<(<Self::ExecutorType as Executor>::Input, Self::Metadata)>>;
}
