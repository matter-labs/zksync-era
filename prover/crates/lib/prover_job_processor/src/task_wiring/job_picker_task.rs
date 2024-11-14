use anyhow::Context;
use async_trait::async_trait;

use crate::{BackoffAndCancellable, Executor, JobPicker, task_wiring::task::Task};

pub struct JobPickerTask<P: JobPicker> {
    picker: P,
    input_tx: tokio::sync::mpsc::Sender<(
        <P::ExecutorType as Executor>::Input,
        <P::ExecutorType as Executor>::Metadata,
    )>,
    backoff_and_cancellable: Option<BackoffAndCancellable>,
}

impl<P: JobPicker> JobPickerTask<P> {
    pub fn new(
        picker: P,
        input_tx: tokio::sync::mpsc::Sender<(
            <P::ExecutorType as Executor>::Input,
            <P::ExecutorType as Executor>::Metadata,
        )>,
        backoff_and_cancellable: Option<BackoffAndCancellable>,
    ) -> Self {
        Self { picker, input_tx, backoff_and_cancellable }
    }

    async fn backoff(&mut self) {
        if let Some(backoff_and_cancellable) = &mut self.backoff_and_cancellable {
            let backoff_duration = backoff_and_cancellable.backoff.delay();
            tracing::info!("Backing off for {:?}...", backoff_duration);
            // Error here corresponds to a timeout w/o receiving task_wiring cancel; we're OK with this.
            tokio::time::timeout(backoff_duration, backoff_and_cancellable.cancellation_token.cancelled())
                .await
                .ok();
        }
    }

    fn reset_backoff(&mut self) {
        if let Some(backoff_and_cancellable) = &mut self.backoff_and_cancellable {
            backoff_and_cancellable.backoff.reset();
        }
    }

    fn is_cancelled(&self) -> bool {
        if let Some(backoff_and_cancellable) = &self.backoff_and_cancellable {
            return backoff_and_cancellable.cancellation_token.is_cancelled();
        }
        false
    }
}

#[async_trait]
impl<P: JobPicker> Task for JobPickerTask<P> {
    async fn run(mut self) -> anyhow::Result<()> {
        while !self.is_cancelled() {
            match self.picker.pick_job().await.context("failed to pick job")? {
                Some((input, metadata)) => {
                    self.input_tx.send((input, metadata)).await.map_err(|err| anyhow::anyhow!("job picker failed to pass job to executor: {}", err))?;
                    self.reset_backoff();
                }
                None => {
                    self.backoff().await;
                }
            }
        }
        tracing::info!("Stop signal received, shutting down JobPickerTask...");
        Ok(())
    }
}
