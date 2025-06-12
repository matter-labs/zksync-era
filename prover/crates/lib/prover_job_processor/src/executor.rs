use std::sync::Arc;

use async_trait::async_trait;

/// Executor trait, responsible for defining what a job's execution will look like.
///
/// The trait covers what it expects as input, what it'll offer as output and what metadata needs to travel together with the input.
/// This is the backbone of the `prover_job_processor` from a user's point of view.
#[async_trait]
pub trait Executor: Send + Sync + 'static {
    type Input: Send;
    type Output: Send;
    type Metadata: Send + Clone;

    fn execute(&self, input: Self::Input, metadata: Self::Metadata)
        -> anyhow::Result<Self::Output>;
        
    async fn execute_async(
        self: Arc<Self>, 
        input: Self::Input, 
        metadata: Self::Metadata,
    ) -> anyhow::Result<Self::Output> {
        let executor = self.clone();
        let exec_metadata = metadata.clone();
        tokio::task::spawn_blocking(move || executor.execute(input, exec_metadata))
            .await
            .expect("failed executing")
    }
}
