use async_trait::async_trait;

use crate::Executor;

#[async_trait]
pub trait JobSaver: Send + Sync + 'static {
    type ExecutorType: Executor;

    async fn save_result(
        &self,
        data: (
            anyhow::Result<<Self::ExecutorType as Executor>::Output>,
            <Self::ExecutorType as Executor>::Metadata,
        ),
    ) -> anyhow::Result<()>;
}
