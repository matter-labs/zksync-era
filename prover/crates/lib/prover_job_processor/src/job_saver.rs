use async_trait::async_trait;

use crate::Executor;

/// Job Saver trait, in charge of getting the result from the executor and dispatching it.
///
/// Dispatch could be storing it, or sending to a separate component.
/// NOTE: Job Savers are tied to an executor, which ensures input/output/metadata types match.
#[async_trait]
pub trait JobSaver: Send + Sync + 'static {
    type ExecutorType: Executor;

    async fn save_job_result(
        &self,
        data: (
            anyhow::Result<<Self::ExecutorType as Executor>::Output>,
            <Self::ExecutorType as Executor>::Metadata,
        ),
    ) -> anyhow::Result<()>;
}
