use std::fmt::Debug;
use std::time::{Duration, Instant};

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
    /// Otherwise, returns Some(job_id, job)
    /// Note: must be concurrency-safe - that is, one job must not be returned in two parallel processes
    async fn get_next_job(&self) -> Option<(Self::JobId, Self::Job)>;

    /// Invoked when `process_job` panics
    /// Should mark the job as failed
    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String);

    /// Function that processes a job
    async fn process_job(
        &self,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<Self::JobArtifacts>;

    /// `iterations_left`:
    /// To run indefinitely, pass `None`,
    /// To process one job, pass `Some(1)`,
    /// To process a batch, pass `Some(batch_size)`.
    async fn run(self, stop_receiver: watch::Receiver<bool>, mut iterations_left: Option<usize>)
    where
        Self: Sized,
    {
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        while iterations_left.map_or(true, |i| i > 0) {
            if *stop_receiver.borrow() {
                vlog::warn!(
                    "Stop signal received, shutting down {} component while waiting for a new job",
                    Self::SERVICE_NAME
                );
                return;
            }
            if let Some((job_id, job)) = Self::get_next_job(&self).await {
                let started_at = Instant::now();
                backoff = Self::POLLING_INTERVAL_MS;
                iterations_left = iterations_left.map(|i| i - 1);

                vlog::debug!(
                    "Spawning thread processing {:?} job with id {:?}",
                    Self::SERVICE_NAME,
                    job_id
                );
                let task = self.process_job(job, started_at).await;

                self.wait_for_task(job_id, started_at, task).await
            } else if iterations_left.is_some() {
                vlog::info!("No more jobs to process. Server can stop now.");
                return;
            } else {
                vlog::trace!("Backing off for {} ms", backoff);
                sleep(Duration::from_millis(backoff)).await;
                backoff = (backoff * Self::BACKOFF_MULTIPLIER).min(Self::MAX_BACKOFF_MS);
            }
        }
        vlog::info!("Requested number of jobs is processed. Server can stop now.")
    }

    /// Polls task handle, saving its outcome.
    async fn wait_for_task(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        task: JoinHandle<Self::JobArtifacts>,
    ) {
        loop {
            vlog::trace!(
                "Polling {} task with id {:?}. Is finished: {}",
                Self::SERVICE_NAME,
                job_id,
                task.is_finished()
            );
            if task.is_finished() {
                let result = task.await;
                match result {
                    Ok(data) => {
                        vlog::debug!(
                            "{} Job {:?} finished successfully",
                            Self::SERVICE_NAME,
                            job_id
                        );
                        self.save_result(job_id, started_at, data).await;
                    }
                    Err(error) => {
                        let error_message = try_extract_panic_message(error);
                        vlog::error!(
                            "Error occurred while processing {} job {:?}: {:?}",
                            Self::SERVICE_NAME,
                            job_id,
                            error_message
                        );
                        self.save_failure(job_id, started_at, error_message).await;
                    }
                }
                break;
            }
            sleep(Duration::from_millis(Self::POLLING_INTERVAL_MS)).await;
        }
    }

    /// Invoked when `process_job` doesn't panic
    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    );
}
