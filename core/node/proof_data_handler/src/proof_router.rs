use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};

pub struct ProofRouter {
    connection_pool: ConnectionPool<Core>,
    acknowledgment_timeout: Duration,
    proving_timeout: Duration,
}

impl ProofRouter {
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        acknowledgment_timeout: Duration,
        proving_timeout: Duration,
    ) -> Self {
        Self {
            connection_pool,
            acknowledgment_timeout,
            proving_timeout,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, proof router is shutting down");
                break;
            }

            self.connection_pool
                .connection()
                .await?
                .eth_proof_manager_dal()
                .fallback_batches(self.acknowledgment_timeout, self.proving_timeout)
                .await?;
        }
        Ok(())
    }
}
