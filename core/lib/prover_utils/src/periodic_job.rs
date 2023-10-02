use std::time::Duration;

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::time::sleep;

#[async_trait]
pub trait PeriodicJob: Sync + Send {
    const SERVICE_NAME: &'static str;

    /// Runs the routine task periodically in [`Self::polling_interval_ms()`] frequency.
    async fn run_routine_task(&mut self) -> anyhow::Result<()>;

    async fn run(mut self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        tracing::info!(
            "Starting periodic job: {} with frequency: {} ms",
            Self::SERVICE_NAME,
            self.polling_interval_ms()
        );
        loop {
            self.run_routine_task()
                .await
                .context("run_routine_task()")?;
            sleep(Duration::from_millis(self.polling_interval_ms())).await;
        }
    }

    fn polling_interval_ms(&self) -> u64;
}
