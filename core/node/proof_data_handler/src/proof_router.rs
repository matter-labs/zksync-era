use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};

pub struct ProofRouter {
    connection_pool: ConnectionPool<Core>,
    acknowledgment_timeout: Duration,
    proving_timeout: Duration,
    picking_timeout: Duration,
}

impl ProofRouter {
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        acknowledgment_timeout: Duration,
        proving_timeout: Duration,
        picking_timeout: Duration,
    ) -> Self {
        Self {
            connection_pool,
            acknowledgment_timeout,
            proving_timeout,
            picking_timeout,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Proof router started");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, proof router is shutting down");
                break;
            }

            let amount = self
                .connection_pool
                .connection()
                .await?
                .proof_manager_dal()
                .fallback_batches(
                    self.acknowledgment_timeout,
                    self.proving_timeout,
                    self.picking_timeout,
                )
                .await?;

            tracing::info!("Fallbacked {} batches with timeouts: acknowledgment timeout: {:?} and proving timeout: {:?}", amount, self.acknowledgment_timeout, self.proving_timeout);

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        Ok(())
    }
}
