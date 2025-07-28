use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L2ChainId;

use crate::{client::BoxedProofManagerClient, types::ProofRequestIdentifier};

pub struct SubmitProofValidationSubmitter {
    client: Box<dyn BoxedProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
}

impl SubmitProofValidationSubmitter {
    pub fn new(
        client: Box<dyn BoxedProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            client,
            connection_pool,
            l2_chain_id,
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                return Ok(());
            }

            if let Err(e) = self.loop_iteration().await {
                tracing::error!("Error submitting proof validation: {e}");
            }

            let duration = Duration::from_secs(10);

            tracing::info!("Sleeping for {} seconds", duration.as_secs());
            tokio::time::sleep(duration).await;
        }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
        let next_batch_to_be_validated = self
            .connection_pool
            .connection()
            .await?
            .proof_manager_dal()
            .get_batch_to_send_validation_result()
            .await?;

        if let Some((batch_number, validation_result)) = next_batch_to_be_validated {
            let proof_request_identifier = ProofRequestIdentifier {
                chain_id: self.l2_chain_id.as_u64(),
                block_number: batch_number.0 as u64,
            };

            match self
                .client
                .submit_proof_validation_result(proof_request_identifier, validation_result)
                .await
            {
                Ok(tx_hash) => {
                    self.connection_pool
                        .connection()
                        .await?
                        .proof_manager_dal()
                        .mark_batch_as_validated(batch_number, tx_hash)
                        .await?;
                    tracing::info!(
                        "Submitted proof validation for batch {} with tx hash {}",
                        batch_number,
                        tx_hash
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to submit proof validation for batch {}: {}",
                        batch_number,
                        e
                    );
                }
            }
        }

        tracing::info!("No batches to validate");

        Ok(())
    }
}
