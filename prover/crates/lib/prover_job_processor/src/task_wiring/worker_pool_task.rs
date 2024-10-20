use std::sync::Arc;

use async_trait::async_trait;

use crate::Executor;
use crate::task_wiring::task::Task;
use crate::task_wiring::worker_pool::WorkerPool;

// WorkerPoolTask implementation
pub struct WorkerPoolTask<E: Executor> {
    executor: Arc<E>,
    num_workers: usize,
    input_rx: tokio::sync::mpsc::Receiver<E::Input>,
    result_tx: tokio::sync::mpsc::Sender<anyhow::Result<E::Output>>,
}

impl<E: Executor> WorkerPoolTask<E> {
    pub fn new(
        executor: Arc<E>,
        num_workers: usize,
        input_rx: tokio::sync::mpsc::Receiver<E::Input>,
        result_tx: tokio::sync::mpsc::Sender<anyhow::Result<E::Output>>,
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
impl<E: Executor> Task for WorkerPoolTask<E>
// where
//     E: Executor,
//     E::Input: Send + 'static,
//     E::Output: Send + 'static,
{
    async fn run(mut self) -> anyhow::Result<()> {
        let WorkerPoolTask {
            executor,
            num_workers,
            input_rx,
            result_tx,
        } = self;

        let worker_pool = WorkerPool::new(executor, num_workers);
        worker_pool.start(input_rx, result_tx).await;

        Ok(())
    }
}