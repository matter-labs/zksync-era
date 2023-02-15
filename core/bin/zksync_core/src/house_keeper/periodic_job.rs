use std::time::Duration;

use async_trait::async_trait;
use tokio::time::sleep;

use zksync_dal::ConnectionPool;

#[async_trait]
pub trait PeriodicJob {
    const SERVICE_NAME: &'static str;
    const POLLING_INTERVAL_MS: u64 = 1000;

    /// Runs the routine task periodically in `POLLING_INTERVAL_MS` frequency.
    fn run_routine_task(&mut self, connection_pool: ConnectionPool);

    async fn run(mut self, connection_pool: ConnectionPool)
    where
        Self: Sized,
    {
        vlog::info!(
            "Starting periodic job: {} with frequency: {} ms",
            Self::SERVICE_NAME,
            Self::POLLING_INTERVAL_MS
        );
        loop {
            self.run_routine_task(connection_pool.clone());
            sleep(Duration::from_millis(Self::POLLING_INTERVAL_MS)).await;
        }
    }
}
