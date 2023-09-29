use std::fmt::Debug;
use std::time::{Duration, Instant};

use anyhow::Context as _;
pub use async_trait::async_trait;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use zksync_utils::panic_extractor::try_extract_panic_message;

#[async_trait]
pub trait JobProcessor: Sync + Send {
    type Job: Send + 'static;
    type JobId: Send + Debug + 'static;
    type JobArtifacts: Send + 'static;

    const POLLING_INTERVAL_MS: u64 = 1000;
    const MAX_BACKOFF_MS: u64 = 60_000;
    const BACKOFF_MULTIPLIER: u64 = 2;
    const SERVICE_NAME: &'static str;

    /// Returns None when there is no pending job
    /// Otherwise, returns Some(job_id, attempts, job)
    /// Note: must be concurrency-safe - that is, one job must not be returned in two parallel processes
    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, u32, Self::Job)>>;

    /// Invoked when `process_job` panics
    /// Should mark the job as failed
    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String);

    /// Function that processes a job
    async fn process_job(
        &self,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>>;

    /// `iterations_left`:
    /// To run indefinitely, pass `None`,
    /// To process one job, pass `Some(1)`,
    /// To process a batch, pass `Some(batch_size)`.
    async fn run(
        self,
        stop_receiver: watch::Receiver<bool>,
        mut iterations_left: Option<usize>,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        while iterations_left.map_or(true, |i| i > 0) {
            if *stop_receiver.borrow() {
                tracing::warn!(
                    "Stop signal received, shutting down {} component while waiting for a new job",
                    Self::SERVICE_NAME
                );
                return Ok(());
            }
            if let Some((job_id, attempts, job)) =
                Self::get_next_job(&self).await.context("get_next_job()")?
            {
                metrics::histogram!(format!("{}.job_attempts", Self::SERVICE_NAME), attempts as f64, "job_id" => format!("{job_id:?}"));
                if attempts == self.max_attempts() {
                    metrics::counter!(format!("{}.max_attempts_reached", Self::SERVICE_NAME), 1, "job_id" => format!("{job_id:?}"));
                }

                let started_at = Instant::now();
                backoff = Self::POLLING_INTERVAL_MS;
                iterations_left = iterations_left.map(|i| i - 1);

                tracing::debug!(
                    "Spawning thread processing {:?} job with id {:?}",
                    Self::SERVICE_NAME,
                    job_id
                );
                let task = self.process_job(job, started_at).await;

                self.wait_for_task(job_id, started_at, task)
                    .await
                    .context("wait_for_task")?;
            } else if iterations_left.is_some() {
                tracing::info!("No more jobs to process. Server can stop now.");
                return Ok(());
            } else {
                tracing::trace!("Backing off for {} ms", backoff);
                sleep(Duration::from_millis(backoff)).await;
                backoff = (backoff * Self::BACKOFF_MULTIPLIER).min(Self::MAX_BACKOFF_MS);
            }
        }
        tracing::info!("Requested number of jobs is processed. Server can stop now.");
        Ok(())
    }

    /// Polls task handle, saving its outcome.
    async fn wait_for_task(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        task: JoinHandle<anyhow::Result<Self::JobArtifacts>>,
    ) -> anyhow::Result<()> {
        let result = loop {
            tracing::trace!(
                "Polling {} task with id {:?}. Is finished: {}",
                Self::SERVICE_NAME,
                job_id,
                task.is_finished()
            );
            if task.is_finished() {
                break task.await;
            }
            sleep(Duration::from_millis(Self::POLLING_INTERVAL_MS)).await;
        };
        let error_message = match result {
            Ok(Ok(data)) => {
                tracing::debug!(
                    "{} Job {:?} finished successfully",
                    Self::SERVICE_NAME,
                    job_id
                );
                return self
                    .save_result(job_id, started_at, data)
                    .await
                    .context("save_result()");
            }
            Ok(Err(error)) => error.to_string(),
            Err(error) => try_extract_panic_message(error),
        };
        tracing::error!(
            "Error occurred while processing {} job {:?}: {:?}",
            Self::SERVICE_NAME,
            job_id,
            error_message
        );
        self.save_failure(job_id, started_at, error_message).await;
        Ok(())
    }

    /// Invoked when `process_job` doesn't panic
    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()>;

    fn max_attempts(&self) -> u32;
}
