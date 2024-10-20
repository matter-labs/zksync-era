use std::sync::Arc;

use async_trait::async_trait;

use crate::{task_wiring::task::Task, Executor, JobSaver};

// SaverTask implementation
pub struct JobSaverTask<S: JobSaver> {
    saver: Arc<S>,
    result_rx: tokio::sync::mpsc::Receiver<(
        anyhow::Result<<S::ExecutorType as Executor>::Output>,
        <S::ExecutorType as Executor>::Metadata,
    )>,
}

impl<S: JobSaver> JobSaverTask<S> {
    pub fn new(
        saver: Arc<S>,
        result_rx: tokio::sync::mpsc::Receiver<(
            anyhow::Result<<S::ExecutorType as Executor>::Output>,
            <S::ExecutorType as Executor>::Metadata,
        )>,
    ) -> Self {
        Self { saver, result_rx }
    }
}

#[async_trait]
impl<S: JobSaver> Task for JobSaverTask<S> {
    async fn run(mut self) -> anyhow::Result<()> {
        while let Some(data) = self.result_rx.recv().await {
            if let Err(e) = self.saver.save_result(data).await {
                eprintln!("Error saving result: {:?}", e);
            }
        }
        Ok(())
    }
}
