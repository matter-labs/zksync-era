use std::sync::Arc;

use crate::{executor::Executor, task_wiring::Task};
use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;
use futures::stream::StreamExt;





#[derive(Debug)]
pub struct AsyncExecutorTask<E>
where
    E: Executor,
{
    executor: E,
    input_rx: tokio::sync::mpsc::Receiver<(E::Input, E::Metadata)>,
    result_tx: tokio::sync::mpsc::Sender<(anyhow::Result<E::Output>, E::Metadata)>,
}

impl<E: Executor> AsyncExecutorTask<E> {
    pub fn new(
        executor: E,
        input_rx: tokio::sync::mpsc::Receiver<(E::Input, E::Metadata)>,
        result_tx: tokio::sync::mpsc::Sender<(anyhow::Result<E::Output>, E::Metadata)>,
    ) -> Self {
        Self {
            executor,
            input_rx,
            result_tx,
        }
    }
}

#[async_trait]
impl<E: Executor> Task for AsyncExecutorTask<E> {
    async fn run(mut self) -> anyhow::Result<()> {
        let executor = Arc::new(self.executor);
        let (input, metadata) = self.input_rx.recv().await
            .expect("input channel has been closed unexpectedly");
        let payload = executor.execute_async(
            input,
            metadata.clone(),
        ).await;
        self.result_tx
            .send((payload, metadata))
            .await
            .expect("job saver channel has been closed unexpectedly");

        Ok(())
    }
}
