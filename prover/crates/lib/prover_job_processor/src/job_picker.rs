use async_trait::async_trait;

use crate::Executor;

/// Job Picker trait, in charge of getting a new job for executor.
/// NOTE: Job Pickers are tied to an executor, which ensures input/output/metadata types match.
#[async_trait]
pub trait JobPicker: Send + Sync + 'static {
    type ExecutorType: Executor;
    async fn pick_job(
        &mut self,
    ) -> anyhow::Result<
        Option<(
            <Self::ExecutorType as Executor>::Input,
            <Self::ExecutorType as Executor>::Metadata,
        )>,
    >;
}
