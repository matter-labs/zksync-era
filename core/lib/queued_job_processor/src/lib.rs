use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use anyhow::Context as _;
pub use async_trait::async_trait;
use tokio::{sync::watch, task::JoinHandle};
use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics};
use zksync_utils::panic_extractor::try_extract_panic_message;

const ATTEMPT_BUCKETS: Buckets = Buckets::exponential(1.0..=64.0, 2.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "job_processor")]
struct JobProcessorMetrics {
    #[metrics(labels = ["service_name", "job_id"])]
    max_attempts_reached: LabeledFamily<(&'static str, String), Counter, 2>,
    #[metrics(labels = ["service_name"], buckets = ATTEMPT_BUCKETS)]
    attempts: LabeledFamily<&'static str, Histogram<usize>>,
}

#[vise::register]
static METRICS: vise::Global<JobProcessorMetrics> = vise::Global::new();

#[async_trait]
pub trait JobProcessor: Sync + Send {
    type Job: Send + 'static;
    type JobId: Send + Sync + Debug + 'static;
    type JobArtifacts: Send + 'static;

    const POLLING_INTERVAL_MS: u64 = 1000;
    const MAX_BACKOFF_MS: u64 = 60_000;
    const BACKOFF_MULTIPLIER: u64 = 2;
    const SERVICE_NAME: &'static str;

    /// Returns None when there is no pending job
    /// Otherwise, returns Some(job_id, job)
    /// Note: must be concurrency-safe - that is, one job must not be returned in two parallel processes
    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>>;

    /// Invoked when `process_job` panics
    /// Should mark the job as failed
    async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String);

    /// Function that processes a job
    async fn process_job(
        &self,
        job_id: &Self::JobId,
        job: Self::Job,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>>;

    /// `iterations_left`:
    /// To run indefinitely, pass `None`,
    /// To process one job, pass `Some(1)`,
    /// To process a batch, pass `Some(batch_size)`.
    async fn run(
        self,
        mut stop_receiver: watch::Receiver<bool>,
        mut iterations_left: Option<usize>,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        while iterations_left.is_none_or(|i| i > 0) {
            if *stop_receiver.borrow() {
                tracing::warn!(
                    "Stop request received, shutting down {} component while waiting for a new job",
                    Self::SERVICE_NAME
                );
                return Ok(());
            }
            if let Some((job_id, job)) =
                Self::get_next_job(&self).await.context("get_next_job()")?
            {
                let started_at = Instant::now();
                backoff = Self::POLLING_INTERVAL_MS;
                iterations_left = iterations_left.map(|i| i - 1);

                tracing::debug!(
                    "Spawning thread processing {:?} job with id {:?}",
                    Self::SERVICE_NAME,
                    job_id
                );
                let task = self.process_job(&job_id, job, started_at).await;

                self.wait_for_task(job_id, started_at, task, &mut stop_receiver)
                    .await
                    .context("wait_for_task")?;
            } else if iterations_left.is_some() {
                tracing::info!("No more jobs to process. Server can stop now.");
                return Ok(());
            } else {
                tracing::trace!("Backing off for {} ms", backoff);
                // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
                tokio::time::timeout(Duration::from_millis(backoff), stop_receiver.changed())
                    .await
                    .ok();
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
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let attempts = self.get_job_attempts(&job_id).await?;
        let max_attempts = self.max_attempts();
        if attempts == max_attempts {
            METRICS.max_attempts_reached[&(Self::SERVICE_NAME, format!("{job_id:?}"))].inc();
            tracing::error!(
                "Max attempts ({max_attempts}) reached for {} job {:?}",
                Self::SERVICE_NAME,
                job_id,
            );
        }

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
            if tokio::time::timeout(
                Duration::from_millis(Self::POLLING_INTERVAL_MS),
                stop_receiver.changed(),
            )
            .await
            .is_ok()
            {
                // Stop request received, return early.
                // Exit will be processed/reported by the main loop.
                return Ok(());
            }
        };
        let error_message = match result {
            Ok(Ok(data)) => {
                tracing::debug!(
                    "{} Job {:?} finished successfully",
                    Self::SERVICE_NAME,
                    job_id
                );
                METRICS.attempts[&Self::SERVICE_NAME].observe(attempts as usize);
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

    /// Invoked in `wait_for_task` for in-progress job.
    async fn get_job_attempts(&self, job_id: &Self::JobId) -> anyhow::Result<u32>;
}
