//! Reports etherscan verifier metrics
use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};

use crate::metrics::CONTRACT_VERIFIER_METRICS;

#[derive(Debug)]
pub struct EtherscanVerifierMetrics {
    connection_pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
}

impl EtherscanVerifierMetrics {
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        stop_receiver: watch::Receiver<bool>,
    ) -> Self {
        Self {
            connection_pool,
            stop_receiver,
        }
    }

    fn update_metrics_interval() -> Duration {
        Duration::from_secs(60)
    }

    async fn update_queued_requests_metric(&self) -> anyhow::Result<()> {
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;
        let number_of_queued_requests = connection
            .etherscan_verification_dal()
            .get_number_of_queued_requests()
            .await?;
        CONTRACT_VERIFIER_METRICS.number_of_queued_requests[&"etherscan_verifier"]
            .set(number_of_queued_requests);
        Ok(())
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            if *self.stop_receiver.borrow() {
                return Ok(());
            }
            if let Err(err) = self.update_queued_requests_metric().await {
                tracing::error!("Failed to update number of queued requests metric: {}", err);
            }
            tokio::time::timeout(
                Self::update_metrics_interval(),
                self.stop_receiver.changed(),
            )
            .await
            .ok();
        }
    }
}
