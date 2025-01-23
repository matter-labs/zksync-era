use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::{executor::Executor, task_wiring::Task};

/// Wrapper over Executor. Makes it a continuous task, picking tasks until picker channel is closed.
/// It can execute multiple concurrent executors, up to specified limit.
#[derive(Debug)]
pub struct WorkerPool<E>
where
    E: Executor,
{
    executor: E,
    num_workers: usize,
    input_rx: tokio::sync::mpsc::Receiver<(E::Input, E::Metadata)>,
    result_tx: tokio::sync::mpsc::Sender<(anyhow::Result<E::Output>, E::Metadata)>,
}

impl<E: Executor> WorkerPool<E> {
    pub fn new(
        executor: E,
        num_workers: usize,
        input_rx: tokio::sync::mpsc::Receiver<(E::Input, E::Metadata)>,
        result_tx: tokio::sync::mpsc::Sender<(anyhow::Result<E::Output>, E::Metadata)>,
    ) -> Self {
        Self {
            executor,
            num_workers,
            input_rx,
            result_tx,
        }
    }
}

#[async_trait]
impl<E: Executor> Task for WorkerPool<E> {
    async fn run(mut self) -> anyhow::Result<()> {
        let executor = Arc::new(self.executor);
        let num_workers = self.num_workers;
        let stream = ReceiverStream::new(self.input_rx);

        stream
            .for_each_concurrent(num_workers, move |(input, metadata)| {
                let executor = executor.clone();
                let result_tx = self.result_tx.clone();
                let exec_metadata = metadata.clone();
                async move {
                    let payload =
                        tokio::task::spawn_blocking(move || executor.execute(input, exec_metadata))
                            .await
                            .expect("failed executing");
                    result_tx
                        .send((payload, metadata))
                        .await
                        .expect("job saver channel has been closed unexpectedly");
                }
            })
            .await;
        Ok(())
    }
}
