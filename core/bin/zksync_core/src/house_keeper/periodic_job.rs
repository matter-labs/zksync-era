use std::time::Duration;

use async_trait::async_trait;
use tokio::time::sleep;

use zksync_dal::ConnectionPool;

#[async_trait]
pub trait PeriodicJob: Sync + Send {
    const SERVICE_NAME: &'static str;

    /// Runs the routine task periodically in [`Self::polling_interval_ms()`] frequency.
    fn run_routine_task(&mut self, connection_pool: ConnectionPool);

    async fn run(mut self, connection_pool: ConnectionPool)
    where
        Self: Sized,
    {
        vlog::info!(
            "Starting periodic job: {} with frequency: {} ms",
            Self::SERVICE_NAME,
            self.polling_interval_ms()
        );
        loop {
            self.run_routine_task(connection_pool.clone());
            sleep(Duration::from_millis(self.polling_interval_ms())).await;
        }
    }

    fn polling_interval_ms(&self) -> u64;
}
