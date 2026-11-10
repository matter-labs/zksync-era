use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L2ChainId;

use crate::{
    client::EthProofManagerClient,
    metrics::{TxType, METRICS},
    types::{decode_proof_request_is_not_proven, ClientError, ProofRequestIdentifier},
};

/// Interval between validation reporting attempts for the same batch.
const VALIDATION_RETRY_BACKOFF: Duration = Duration::from_secs(600);

pub struct SubmitProofValidationSubmitter {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
    max_attempts: u64,
    last_reported_stuck: AtomicU64,
}

impl SubmitProofValidationSubmitter {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        l2_chain_id: L2ChainId,
        max_attempts: u64,
    ) -> Self {
        Self {
            client,
            connection_pool,
            l2_chain_id,
            max_attempts,
            last_reported_stuck: AtomicU64::new(0),
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                return Ok(());
            }

            if let Err(e) = self.loop_iteration().await {
                tracing::error!("Error submitting proof validation: {e:#}");
            }

            let duration = Duration::from_secs(10);

            tracing::info!("Sleeping for {} seconds", duration.as_secs());
            tokio::time::sleep(duration).await;
        }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
        let reached_max_attempts_amount = self
            .connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .get_reached_max_attempts_amount(self.max_attempts)
            .await?;

        METRICS.reached_max_attempts[&TxType::ValidationResult].set(reached_max_attempts_amount);

        // Those batches are no longer selected below, so nothing else will ever mention
        // them again. Logged once per change rather than once per iteration.
        let last_reported = self
            .last_reported_stuck
            .swap(reached_max_attempts_amount, Ordering::Relaxed);
        if reached_max_attempts_amount > 0 && reached_max_attempts_amount != last_reported {
            tracing::error!(
                "{reached_max_attempts_amount} batches exhausted their {} validation \
                 attempts and are no longer retried; their rewards stay locked until \
                 someone intervenes",
                self.max_attempts
            );
        }

        let next_batch_to_be_validated = self
            .connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .get_batch_to_send_validation_result(self.max_attempts, VALIDATION_RETRY_BACKOFF)
            .await?;

        let Some((batch_number, validation_result)) = next_batch_to_be_validated else {
            tracing::info!("No batches to validate");
            return Ok(());
        };

        let proof_request_identifier = ProofRequestIdentifier {
            chain_id: self.l2_chain_id.as_u64(),
            block_number: batch_number.0 as u64,
        };

        let result = self
            .client
            .submit_proof_validation_result(proof_request_identifier, validation_result)
            .await;

        // A request that already carries a validation result on chain rejects any further
        // one. No reward is locked in that state, and no transaction of ours can ever
        // land, so record it as reported instead of reverting once per backoff forever.
        if let Err(ClientError::Reverted {
            tx_hash,
            revert_data,
            ..
        }) = &result
        {
            if let Some(recorded_verdict) = decode_proof_request_is_not_proven(revert_data)
                .and_then(|status| status.recorded_verdict())
            {
                // The recorded verdict disagreeing with ours is an integrity alarm, not
                // backlog: either an invalid proof was accepted and paid for, or a proving
                // network was denied a reward it earned. Both mean a rogue or stale
                // submitter key, or node state diverging from the chain.
                if recorded_verdict != validation_result {
                    METRICS.validation_verdict_mismatch.inc();
                    tracing::error!(
                        "Proof request for batch {batch_number} is recorded on chain with \
                         verdict {recorded_verdict}, but we are reporting {validation_result}. \
                         Someone else holds the submitter role, or our state has diverged."
                    );
                } else {
                    tracing::warn!(
                        "Proof request for batch {batch_number} already has a validation \
                         result on chain (tx {} reverted); marking it as reported",
                        hex::encode(tx_hash)
                    );
                }
                METRICS.validation_already_recorded.inc();

                self.connection_pool
                    .connection()
                    .await?
                    .eth_proof_manager_dal()
                    .mark_batch_validation_already_recorded(batch_number)
                    .await?;

                return Ok(());
            }
        }

        let tx_hash = result.with_context(|| {
            format!("failed to submit proof validation for batch {batch_number}")
        })?;

        self.connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .mark_batch_as_validated(batch_number, tx_hash)
            .await?;
        tracing::info!(
            "Submitted proof validation for batch {} with tx hash {}",
            batch_number,
            tx_hash
        );

        Ok(())
    }
}
