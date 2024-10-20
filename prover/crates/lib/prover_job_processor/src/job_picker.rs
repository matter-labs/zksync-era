use async_trait::async_trait;

use crate::Executor;

#[async_trait]
pub trait JobPicker: Send + Sync + 'static {
    type ExecutorType: Executor;
    async fn pick_job(
        &self,
    ) -> anyhow::Result<
        Option<(
            <Self::ExecutorType as Executor>::Input,
            <Self::ExecutorType as Executor>::Metadata,
        )>,
    >;
}
