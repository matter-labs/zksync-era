use std::sync::Arc;

use async_trait::async_trait;

use crate::{task_wiring::task::Task, Executor, JobPicker};

pub struct JobPickerTask<P: JobPicker> {
    picker: Arc<P>,
    input_tx: tokio::sync::mpsc::Sender<(
        <P::ExecutorType as Executor>::Input,
        <P::ExecutorType as Executor>::Metadata,
    )>,
}

impl<P: JobPicker> JobPickerTask<P> {
    pub fn new(
        picker: Arc<P>,
        input_tx: tokio::sync::mpsc::Sender<(
            <P::ExecutorType as Executor>::Input,
            <P::ExecutorType as Executor>::Metadata,
        )>,
    ) -> Self {
        Self { picker, input_tx }
    }
}

#[async_trait]
impl<P: JobPicker> Task for JobPickerTask<P> {
    async fn run(mut self) -> anyhow::Result<()> {
        loop {
            match self.picker.pick_job().await {
                Ok(Some((input, metadata))) => {
                    if self.input_tx.send((input, metadata)).await.is_err() {
                        // Worker pool has been dropped
                        break;
                    }
                }
                Ok(None) => {
                    // No job available, sleep and retry
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
                Err(e) => {
                    eprintln!("Error picking job: {:?}", e);
                    // Sleep and retry
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
        // Close the input channel when done
        drop(self.input_tx.clone());
        Ok(())
    }
}
