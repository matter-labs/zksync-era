use std::{fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::ConnectionPool;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{L1BatchNumber, MiniblockNumber, H256};
use zksync_web3_decl::{
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    jsonrpsee::{core::ClientError as RpcError, http_client::HttpClient},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use crate::{
    metrics::{CheckerComponent, EN_METRICS},
    utils::{binary_search_with, wait_for_l1_batch_with_metadata},
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum HashMatchError {
    #[error("RPC error calling main node")]
    Rpc(#[from] EnrichedClientError),
    #[error(
        "Unrecoverable error: the earliest L1 batch #{0} in the local DB \
        has mismatched hash with the main node. Make sure you're connected to the right network; \
        if you've recovered from a snapshot, re-check snapshot authenticity. \
        Using an earlier snapshot could help."
    )]
    EarliestHashMismatch(L1BatchNumber),
    #[error(
        "Unrecoverable error: the earliest L1 batch #{0} in the local DB \
        is truncated on the main node. Make sure you're connected to the right network; \
        if you've recovered from a snapshot, re-check snapshot authenticity. \
        Using an earlier snapshot could help."
    )]
    EarliestL1BatchTruncated(L1BatchNumber),
    #[error("reorg detected, restart the node to revert to the last correct L1 batch #{0}.")]
    ReorgDetected(L1BatchNumber),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::SqlxError> for HashMatchError {
    fn from(err: zksync_dal::SqlxError) -> Self {
        Self::Internal(err.into())
    }
}

fn is_transient_err(err: &EnrichedClientError) -> bool {
    matches!(
        err.as_ref(),
        RpcError::Transport(_) | RpcError::RequestTimeout
    )
}

#[async_trait]
trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn sealed_miniblock_number(&self) -> EnrichedClientResult<MiniblockNumber>;

    async fn sealed_l1_batch_number(&self) -> EnrichedClientResult<L1BatchNumber>;

    async fn miniblock_hash(&self, number: MiniblockNumber) -> EnrichedClientResult<Option<H256>>;

    async fn l1_batch_root_hash(&self, number: L1BatchNumber)
        -> EnrichedClientResult<Option<H256>>;
}

#[async_trait]
impl MainNodeClient for HttpClient {
    async fn sealed_miniblock_number(&self) -> EnrichedClientResult<MiniblockNumber> {
        let number = self
            .get_block_number()
            .rpc_context("sealed_miniblock_number")
            .await?;
        let number = u32::try_from(number).map_err(|err| {
            EnrichedClientError::custom(err, "u32::try_from").with_arg("number", &number)
        })?;
        Ok(MiniblockNumber(number))
    }

    async fn sealed_l1_batch_number(&self) -> EnrichedClientResult<L1BatchNumber> {
        let number = self
            .get_l1_batch_number()
            .rpc_context("sealed_l1_batch_number")
            .await?;
        let number = u32::try_from(number).map_err(|err| {
            EnrichedClientError::custom(err, "u32::try_from").with_arg("number", &number)
        })?;
        Ok(L1BatchNumber(number))
    }

    async fn miniblock_hash(&self, number: MiniblockNumber) -> EnrichedClientResult<Option<H256>> {
        Ok(self
            .get_block_by_number(number.0.into(), false)
            .rpc_context("miniblock_hash")
            .with_arg("number", &number)
            .await?
            .map(|block| block.hash))
    }

    async fn l1_batch_root_hash(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<H256>> {
        Ok(self
            .get_l1_batch_details(number)
            .rpc_context("l1_batch_root_hash")
            .with_arg("number", &number)
            .await?
            .and_then(|batch| batch.base.root_hash))
    }
}

trait HandleReorgDetectorEvent: fmt::Debug + Send + Sync {
    fn initialize(&mut self);

    fn update_correct_block(
        &mut self,
        last_correct_miniblock: MiniblockNumber,
        last_correct_l1_batch: L1BatchNumber,
    );

    fn report_divergence(&mut self, diverged_l1_batch: L1BatchNumber);

    fn start_shutting_down(&mut self);
}

/// Default implementation of [`HandleReorgDetectorEvent`] that reports values as metrics.
impl HandleReorgDetectorEvent for HealthUpdater {
    fn initialize(&mut self) {
        self.update(Health::from(HealthStatus::Ready));
    }

    fn update_correct_block(
        &mut self,
        last_correct_miniblock: MiniblockNumber,
        last_correct_l1_batch: L1BatchNumber,
    ) {
        let last_correct_miniblock = last_correct_miniblock.0.into();
        let prev_checked_miniblock = EN_METRICS.last_correct_miniblock
            [&CheckerComponent::ReorgDetector]
            .set(last_correct_miniblock);
        if prev_checked_miniblock != last_correct_miniblock {
            tracing::debug!("No reorg at miniblock #{last_correct_miniblock}");
        }

        let last_correct_l1_batch = last_correct_l1_batch.0.into();
        let prev_checked_l1_batch = EN_METRICS.last_correct_batch[&CheckerComponent::ReorgDetector]
            .set(last_correct_l1_batch);
        if prev_checked_l1_batch != last_correct_l1_batch {
            tracing::debug!("No reorg at L1 batch #{last_correct_l1_batch}");
        }

        let health_details = serde_json::json!({
            "last_correct_miniblock": last_correct_miniblock,
            "last_correct_l1_batch": last_correct_l1_batch,
        });
        self.update(Health::from(HealthStatus::Ready).with_details(health_details));
    }

    fn report_divergence(&mut self, diverged_l1_batch: L1BatchNumber) {
        let health_details = serde_json::json!({
            "diverged_l1_batch": diverged_l1_batch,
        });
        self.update(Health::from(HealthStatus::Affected).with_details(health_details));
    }

    fn start_shutting_down(&mut self) {
        self.update(HealthStatus::ShuttingDown.into());
    }
}

/// Output of hash match methods in [`ReorgDetector`].
#[derive(Debug)]
enum MatchOutput {
    Match,
    Mismatch,
    NoRemoteReference,
}

impl MatchOutput {
    fn new(is_match: bool) -> Self {
        if is_match {
            Self::Match
        } else {
            Self::Mismatch
        }
    }
}

/// This is a component that is responsible for detecting the batch re-orgs.
/// Batch re-org is a rare event of manual intervention, when the node operator
/// decides to revert some of the not yet finalized batches for some reason
/// (e.g. inability to generate a proof), and then potentially
/// re-organize transactions in them to fix the problem.
///
/// To detect them, we constantly check the latest sealed batch root hash,
/// and in the event of mismatch, we know that there has been a re-org.
/// We then perform a binary search to find the latest correct block
/// and revert all batches after it, to keep being consistent with the main node.
///
/// This is the only component that is expected to finish its execution
/// in the event of re-org, since we have to restart the node after a rollback is performed,
/// and is special-cased in the `zksync_external_node` crate.
#[derive(Debug)]
pub struct ReorgDetector {
    client: Box<dyn MainNodeClient>,
    event_handler: Box<dyn HandleReorgDetectorEvent>,
    pool: ConnectionPool,
    sleep_interval: Duration,
    health_check: ReactiveHealthCheck,
}

impl ReorgDetector {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(client: HttpClient, pool: ConnectionPool) -> Self {
        let (health_check, health_updater) = ReactiveHealthCheck::new("reorg_detector");
        Self {
            client: Box::new(client),
            event_handler: Box::new(health_updater),
            pool,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
            health_check,
        }
    }

    pub fn health_check(&self) -> &ReactiveHealthCheck {
        &self.health_check
    }

    /// Returns `Ok(None)` if no reorg was detected.
    /// Returns `Ok(Some(n))` if a reorg was detected and `n` is the last
    /// correct l1 batch.
    pub async fn should_revert(&mut self) -> anyhow::Result<Option<L1BatchNumber>> {
        let mut storage = self.pool.access_storage().await?;
        let Some(local_l1_batch) = storage
            .blocks_dal()
            .get_last_l1_batch_number_with_metadata()
            .await?
        else {
            return Ok(None);
        };
        let Some(local_miniblock) = storage.blocks_dal().get_sealed_miniblock_number().await?
        else {
            return Ok(None);
        };
        drop(storage);

        let remote_l1_batch = self.client.sealed_l1_batch_number().await?;
        let remote_miniblock = self.client.sealed_miniblock_number().await?;

        let checked_l1_batch = local_l1_batch.min(remote_l1_batch);
        let checked_miniblock = local_miniblock.min(remote_miniblock);

        let root_hashes_match = match self.root_hashes_match(checked_l1_batch).await? {
            MatchOutput::Match => true,
            MatchOutput::Mismatch => false,
            MatchOutput::NoRemoteReference => anyhow::bail!("remote L1 batch missing"),
        };
        let miniblock_hashes_match = match self.miniblock_hashes_match(checked_miniblock).await? {
            MatchOutput::Match => true,
            MatchOutput::Mismatch => false,
            MatchOutput::NoRemoteReference => anyhow::bail!("remote miniblock missing"),
        };

        // The only event that triggers re-org detection and node rollback is if the
        // hash mismatch at the same block height is detected, be it miniblocks or batches.
        //
        // In other cases either there is only a height mismatch which means that one of
        // the nodes needs to do catching up; however, it is not certain that there is actually
        // a re-org taking place.
        if root_hashes_match && miniblock_hashes_match {
            self.event_handler
                .update_correct_block(checked_miniblock, checked_l1_batch);
            return Ok(None);
        }
        let diverged_l1_batch = checked_l1_batch + (root_hashes_match as u32);
        self.event_handler.report_divergence(diverged_l1_batch);

        tracing::info!("Searching for the first diverged L1 batch");
        let mut storage = self.pool.access_storage().await?;
        let earliest_l1_batch = storage
            .blocks_dal()
            .get_earliest_l1_batch_number_with_metadata()
            .await?
            .context("all L1 batches with metadata disappeared")?;
        drop(storage);
        let last_correct_l1_batch = self
            .detect_reorg(earliest_l1_batch, diverged_l1_batch)
            .await?;
        tracing::info!("Reorg localized: last correct L1 batch is #{last_correct_l1_batch}");
        Ok(Some(last_correct_l1_batch))
    }

    /// Compares hashes of the given local miniblock and the same miniblock from main node.
    async fn miniblock_hashes_match(
        &self,
        miniblock_number: MiniblockNumber,
    ) -> Result<MatchOutput, HashMatchError> {
        let mut storage = self.pool.access_storage().await?;
        let local_hash = storage
            .blocks_dal()
            .get_miniblock_header(miniblock_number)
            .await?
            .with_context(|| {
                format!("Header does not exist for local miniblock #{miniblock_number}")
            })?
            .hash;
        drop(storage);

        let Some(remote_hash) = self.client.miniblock_hash(miniblock_number).await? else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            return Ok(MatchOutput::NoRemoteReference);
        };

        if remote_hash != local_hash {
            tracing::warn!(
                "Reorg detected: local hash {local_hash:?} doesn't match the hash from \
                main node {remote_hash:?} (miniblock #{miniblock_number})"
            );
        }
        Ok(MatchOutput::new(remote_hash == local_hash))
    }

    /// Compares root hashes of the latest local batch and of the same batch from the main node.
    async fn root_hashes_match(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<MatchOutput, HashMatchError> {
        let mut storage = self.pool.access_storage().await?;
        let local_hash = storage
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch_number)
            .await?
            .with_context(|| {
                format!("Root hash does not exist for local batch #{l1_batch_number}")
            })?;
        drop(storage);

        let Some(remote_hash) = self.client.l1_batch_root_hash(l1_batch_number).await? else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the root hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            return Ok(MatchOutput::NoRemoteReference);
        };

        if remote_hash != local_hash {
            tracing::warn!(
                "Reorg detected: local root hash {local_hash:?} doesn't match the state hash from \
                main node {remote_hash:?} (L1 batch #{l1_batch_number})"
            );
        }
        Ok(MatchOutput::new(remote_hash == local_hash))
    }

    /// Localizes a re-org: performs binary search to determine the last non-diverged block.
    async fn detect_reorg(
        &self,
        known_valid_l1_batch: L1BatchNumber,
        diverged_l1_batch: L1BatchNumber,
    ) -> Result<L1BatchNumber, HashMatchError> {
        // TODO (BFT-176, BFT-181): We have to look through the whole history, since batch status updater may mark
        //   a block as executed even if the state diverges for it.
        binary_search_with(
            known_valid_l1_batch.0,
            diverged_l1_batch.0,
            |number| async move {
                Ok(match self.root_hashes_match(L1BatchNumber(number)).await? {
                    MatchOutput::Match | MatchOutput::NoRemoteReference => true,
                    MatchOutput::Mismatch => false,
                })
            },
        )
        .await
        .map(L1BatchNumber)
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.event_handler.initialize();
        loop {
            match self.run_inner(&mut stop_receiver).await {
                Ok(l1_batch_number) => return Ok(l1_batch_number),
                Err(HashMatchError::Rpc(err)) if is_transient_err(&err) => {
                    tracing::warn!("Following transport error occurred: {err}");
                    tracing::info!("Trying again after a delay");
                    tokio::time::sleep(self.sleep_interval).await;
                }
                Err(HashMatchError::Internal(err)) => return Err(err),
                Err(err) => return Err(err.into()),
            }
        }
    }

    async fn run_inner(
        &mut self,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<(), HashMatchError> {
        let earliest_l1_batch_number =
            wait_for_l1_batch_with_metadata(&self.pool, self.sleep_interval, stop_receiver).await?;

        let Some(earliest_l1_batch_number) = earliest_l1_batch_number else {
            return Ok(()); // Stop signal received
        };
        tracing::debug!(
            "Checking root hash match for earliest L1 batch #{earliest_l1_batch_number}"
        );
        match self.root_hashes_match(earliest_l1_batch_number).await? {
            MatchOutput::Match => { /* we're good */ }
            MatchOutput::Mismatch => {
                return Err(HashMatchError::EarliestHashMismatch(
                    earliest_l1_batch_number,
                ))
            }
            MatchOutput::NoRemoteReference => {
                return Err(HashMatchError::EarliestL1BatchTruncated(
                    earliest_l1_batch_number,
                ))
            }
        }

        loop {
            if let Some(last_correct_l1_batch) =
                self.should_revert().await.context("should_revert()")?
            {
                return Err(HashMatchError::ReorgDetected(last_correct_l1_batch));
            }
            if *stop_receiver.borrow() {
                self.event_handler.start_shutting_down();
                tracing::info!("Shutting down reorg detector");
                return Ok(());
            }
            tokio::time::sleep(self.sleep_interval).await;
        }
    }
}
