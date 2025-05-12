use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use tracing::Instrument;

#[async_trait]
pub trait PeriodicJob: Sync + Send {
    const SERVICE_NAME: &'static str;

    /// Runs the routine task periodically in [`Self::polling_interval_ms()`] frequency.
    async fn run_routine_task(&mut self) -> anyhow::Result<()>;

    async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let timeout = Duration::from_millis(self.polling_interval_ms());
        tracing::info!(
            "Starting periodic job: {} with frequency: {timeout:?}",
            Self::SERVICE_NAME
        );

        while !*stop_receiver.borrow_and_update() {
            self.run_routine_task()
                .instrument(
                    tracing::info_span!("run_routine_task", service_name = %Self::SERVICE_NAME),
                )
                .await
                .context("run_routine_task()")?;
            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(timeout, stop_receiver.changed())
                .await
                .ok();
        }
        tracing::info!(
            "Stop request received; periodic job {} is shut down",
            Self::SERVICE_NAME
        );
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64;
}
