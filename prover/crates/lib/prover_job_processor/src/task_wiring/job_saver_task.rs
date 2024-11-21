use anyhow::Context;
use async_trait::async_trait;

use crate::{task_wiring::task::Task, JobSaver, Output, SaverMetadata};

/// Wrapper over JobSaver. Makes it a continuous task, picking tasks until execution channel is closed.
#[derive(Debug)]
pub struct JobSaverTask<S: JobSaver> {
    saver: S,
    result_rx: tokio::sync::mpsc::Receiver<(anyhow::Result<Output<S>>, SaverMetadata<S>)>,
}

impl<S: JobSaver> JobSaverTask<S> {
    pub fn new(
        saver: S,
        result_rx: tokio::sync::mpsc::Receiver<(anyhow::Result<Output<S>>, SaverMetadata<S>)>,
    ) -> Self {
        Self { saver, result_rx }
    }
}

#[async_trait]
impl<S: JobSaver> Task for JobSaverTask<S> {
    async fn run(mut self) -> anyhow::Result<()> {
        while let Some(data) = self.result_rx.recv().await {
            self.saver
                .save_job_result(data)
                .await
                .context("failed to save result")?;
        }
        Ok(())
    }
}
