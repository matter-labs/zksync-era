use std::{convert::Infallible, fmt, future::Future, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal, DalError};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_shared_metrics::{CheckerComponent, EN_METRICS};
use zksync_types::{L1BatchNumber, L2BlockNumber, OrStopped, H256};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

pub mod node;
#[cfg(test)]
mod tests;

/// Data missing on the main node.
#[derive(Debug, thiserror::Error)]
#[cfg_attr(test, derive(Clone, Copy))] // deriving these traits for errors is usually a bad idea, so we scope derivation to tests
pub enum MissingData {
    /// The main node lacks a requested L2 block.
    #[error("no requested L2 block")]
    L2Block,
    /// The main node lacks a requested L1 batch.
    #[error("no requested L1 batch")]
    Batch,
    /// The main node lacks a root hash for a requested L1 batch; the batch itself is present on the node.
    #[error("no root hash for L1 batch")]
    RootHash,
    /// The main node lacks a root hash for a requested L1 batch; the batch itself is present on the node.
    #[error("no commitment for L1 batch")]
    Commitment,
}

#[derive(Debug, thiserror::Error)]
pub enum HashMatchError {
    #[error("RPC error calling main node")]
    Rpc(#[from] EnrichedClientError),
    #[error("missing data on main node")]
    MissingData(#[from] MissingData),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<DalError> for HashMatchError {
    fn from(err: DalError) -> Self {
        Self::Internal(err.generalize())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    HashMatch(#[from] HashMatchError),
    #[error(
        "Unrecoverable error: the earliest L1 batch #{0} in the local DB \
        has mismatched hash with the main node. Make sure you're connected to the right network; \
        if you've recovered from a snapshot, re-check snapshot authenticity. \
        Using an earlier snapshot could help."
    )]
    EarliestL1BatchMismatch(L1BatchNumber),
    #[error(
        "Unrecoverable error: the earliest L1 batch #{0} in the local DB \
        is truncated on the main node. Make sure you're connected to the right network; \
        if you've recovered from a snapshot, re-check snapshot authenticity. \
        Using an earlier snapshot could help."
    )]
    EarliestL1BatchTruncated(L1BatchNumber),
    #[error("reorg detected, restart the node to revert to the last correct L1 batch #{0}.")]
    ReorgDetected(L1BatchNumber),
}

impl HashMatchError {
    pub fn is_retriable(&self) -> bool {
        match self {
            Self::Rpc(err) => err.is_retryable(),
            Self::MissingData(_) => true,
            Self::Internal(_) => false,
        }
    }
}

impl Error {
    pub fn is_retriable(&self) -> bool {
        matches!(self, Self::HashMatch(err) if err.is_retriable())
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self::HashMatch(HashMatchError::Internal(err))
    }
}

impl From<DalError> for Error {
    fn from(err: DalError) -> Self {
        Self::HashMatch(HashMatchError::Internal(err.generalize()))
    }
}

impl From<EnrichedClientError> for Error {
    fn from(err: EnrichedClientError) -> Self {
        Self::HashMatch(HashMatchError::Rpc(err))
    }
}

#[async_trait]
trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn sealed_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber>;

    async fn sealed_l1_batch_number(&self) -> EnrichedClientResult<L1BatchNumber>;

    async fn l2_block_hash(&self, number: L2BlockNumber) -> EnrichedClientResult<Option<H256>>;

    async fn l1_batch_root_hash(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>>;

    async fn l1_batch_commitment(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>>;
}

#[async_trait]
impl MainNodeClient for Box<DynClient<L2>> {
    async fn sealed_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber> {
        let number = self
            .get_block_number()
            .rpc_context("sealed_l2_block_number")
            .await?;
        let number = u32::try_from(number).map_err(|err| {
            EnrichedClientError::custom(err, "u32::try_from").with_arg("number", &number)
        })?;
        Ok(L2BlockNumber(number))
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

    async fn l2_block_hash(&self, number: L2BlockNumber) -> EnrichedClientResult<Option<H256>> {
        Ok(self
            .get_block_by_number(number.0.into(), false)
            .rpc_context("l2_block_hash")
            .with_arg("number", &number)
            .await?
            .map(|block| block.hash))
    }

    async fn l1_batch_root_hash(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>> {
        let Some(batch) = self
            .get_l1_batch_details(number)
            .rpc_context("l1_batch_root_hash")
            .with_arg("number", &number)
            .await?
        else {
            return Ok(Err(MissingData::Batch));
        };
        Ok(batch.base.root_hash.ok_or(MissingData::RootHash))
    }

    async fn l1_batch_commitment(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>> {
        let Some(batch) = self
            .get_l1_batch_details(number)
            .rpc_context("l1_batch_commitment")
            .with_arg("number", &number)
            .await?
        else {
            return Ok(Err(MissingData::Batch));
        };
        Ok(batch.commitment.ok_or(MissingData::Commitment))
    }
}

trait HandleReorgDetectorEvent: fmt::Debug + Send + Sync {
    fn initialize(&mut self);

    fn update_correct_block(
        &mut self,
        last_correct_l2_block: L2BlockNumber,
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
        last_correct_l2_block: L2BlockNumber,
        last_correct_l1_batch: L1BatchNumber,
    ) {
        let last_correct_l2_block = last_correct_l2_block.0.into();
        let prev_checked_l2_block = EN_METRICS.last_correct_l2_block
            [&CheckerComponent::ReorgDetector]
            .set(last_correct_l2_block);
        if prev_checked_l2_block != last_correct_l2_block {
            tracing::debug!("No reorg at L2 block #{last_correct_l2_block}");
        }

        let last_correct_l1_batch = last_correct_l1_batch.0.into();
        let prev_checked_l1_batch = EN_METRICS.last_correct_batch[&CheckerComponent::ReorgDetector]
            .set(last_correct_l1_batch);
        if prev_checked_l1_batch != last_correct_l1_batch {
            tracing::debug!("No reorg at L1 batch #{last_correct_l1_batch}");
        }

        let health_details = serde_json::json!({
            "last_correct_l2_block": last_correct_l2_block,
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

/// This is a component that is responsible for detecting the batch re-orgs.
///
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
    pool: ConnectionPool<Core>,
    sleep_interval: Duration,
    health_check: ReactiveHealthCheck,
}

impl ReorgDetector {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(client: Box<DynClient<L2>>, pool: ConnectionPool<Core>) -> Self {
        let (health_check, health_updater) = ReactiveHealthCheck::new("reorg_detector");
        Self {
            client: Box::new(client.for_component("reorg_detector")),
            event_handler: Box::new(health_updater),
            pool,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
            health_check,
        }
    }

    pub fn health_check(&self) -> &ReactiveHealthCheck {
        &self.health_check
    }

    async fn find_last_diverged_batch(&mut self) -> Result<Option<L1BatchNumber>, HashMatchError> {
        let mut storage = self.pool.connection().await?;
        // Create a readonly transaction to get a consistent view of the storage.
        let mut storage_tx = storage
            .transaction_builder()?
            .set_readonly()
            .build()
            .await?;
        let Some(local_l1_batch) = storage_tx
            .blocks_dal()
            .get_last_l1_batch_number_with_commitment()
            .await?
        else {
            return Ok(None);
        };
        let Some(local_l2_block) = storage_tx.blocks_dal().get_sealed_l2_block_number().await?
        else {
            return Ok(None);
        };
        drop(storage_tx);
        drop(storage);

        let remote_l1_batch = self.client.sealed_l1_batch_number().await?;
        let remote_l2_block = self.client.sealed_l2_block_number().await?;
        let checked_l1_batch = local_l1_batch.min(remote_l1_batch);
        let checked_l2_block = local_l2_block.min(remote_l2_block);
        let root_hashes_match = self.root_hashes_match(checked_l1_batch).await?;
        let l2_block_hashes_match = self.l2_block_hashes_match(checked_l2_block).await?;
        let commitments_match = self.commitment_match(checked_l1_batch).await?;

        // The events that triggers re-org detection and node rollback are if the
        // hash mismatch at the same block height is detected, be it L2 blocks or batches or commitment mismatch for l1 batch.
        //
        // In other cases either there is only a height mismatch which means that one of
        // the nodes needs to do catching up; however, it is not certain that there is actually
        // a re-org taking place.
        Ok(
            if root_hashes_match && l2_block_hashes_match && commitments_match {
                self.event_handler
                    .update_correct_block(checked_l2_block, checked_l1_batch);
                None
            } else {
                let diverged_l1_batch = checked_l1_batch;
                self.event_handler.report_divergence(diverged_l1_batch);
                Some(diverged_l1_batch)
            },
        )
    }

    async fn check_consistency(&mut self) -> Result<(), Error> {
        let Some(diverged_l1_batch) = self.find_last_diverged_batch().await? else {
            return Ok(());
        };

        // Check that the first L1 batch matches, to make sure that
        // we are actually tracking the same chain as the main node.
        let mut storage = self.pool.connection().await?;
        let first_l1_batch = storage
            .blocks_dal()
            .get_earliest_l1_batch_number_with_commitment()
            .await?
            .context("all L1 batches disappeared")?;
        drop(storage);
        match self.root_hashes_match(first_l1_batch).await {
            Ok(true) => {}
            Ok(false) => return Err(Error::EarliestL1BatchMismatch(first_l1_batch)),
            Err(HashMatchError::MissingData(_)) => {
                return Err(Error::EarliestL1BatchTruncated(first_l1_batch));
            }
            Err(err) => return Err(err.into()),
        }

        tracing::info!("Searching for the first diverged L1 batch");
        let last_correct_l1_batch = self.detect_reorg(first_l1_batch, diverged_l1_batch).await?;
        tracing::info!("Reorg localized: last correct L1 batch is #{last_correct_l1_batch}");
        Err(Error::ReorgDetected(last_correct_l1_batch))
    }

    /// Compares hashes of the given local L2 block and the same L2 block from main node.
    async fn l2_block_hashes_match(&self, l2_block: L2BlockNumber) -> Result<bool, HashMatchError> {
        let mut storage = self.pool.connection().await?;
        let local_hash = storage
            .blocks_dal()
            .get_l2_block_header(l2_block)
            .await?
            .with_context(|| format!("Header does not exist for local L2 block #{l2_block}"))?
            .hash;
        drop(storage);

        let Some(remote_hash) = self.client.l2_block_hash(l2_block).await? else {
            // Due to reorg, locally we may be ahead of the main node.
            // Lack of the hash on the main node is treated as a hash match,
            // We need to wait for our knowledge of main node to catch up.
            tracing::info!("Remote L2 block #{l2_block} is missing");
            return Err(MissingData::L2Block.into());
        };

        if remote_hash != local_hash {
            tracing::warn!(
                "Reorg detected: local hash {local_hash:?} doesn't match the hash from \
                main node {remote_hash:?} (L2 block #{l2_block})"
            );
        }
        Ok(remote_hash == local_hash)
    }

    /// Compares root hashes of the latest local batch and of the same batch from the main node.
    async fn root_hashes_match(&self, l1_batch: L1BatchNumber) -> Result<bool, HashMatchError> {
        let mut storage = self.pool.connection().await?;
        let local_hash = storage
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch)
            .await?
            .with_context(|| format!("Root hash does not exist for local batch #{l1_batch}"))?;
        drop(storage);

        let remote_hash = self.client.l1_batch_root_hash(l1_batch).await??;
        if remote_hash != local_hash {
            tracing::warn!(
                "Reorg detected: local root hash {local_hash:?} doesn't match the state hash from \
                main node {remote_hash:?} (L1 batch #{l1_batch})"
            );
        }
        Ok(remote_hash == local_hash)
    }

    /// Compares root hashes of the latest local batch and of the same batch from the main node.
    async fn commitment_match(&self, l1_batch: L1BatchNumber) -> Result<bool, HashMatchError> {
        let remote_commitment = self.client.l1_batch_commitment(l1_batch).await?;

        let Ok(remote_commitment) = remote_commitment else {
            // If the commitment is missing on the main node (the version is not yet expose the commitment),
            // we treat it as a match.
            return Ok(true);
        };

        let mut storage = self.pool.connection().await?;
        let local_commitment = storage
            .blocks_dal()
            .get_commitment_for_l1_batch(l1_batch)
            .await?
            .context(format!(
                "Commitment does not exist for local batch #{l1_batch}"
            ))?;
        drop(storage);

        if remote_commitment != local_commitment {
            tracing::warn!(
                "Reorg detected: local commitment {local_commitment:?} doesn't match the state hash from \
                main node {remote_commitment:?} (L1 batch #{l1_batch})"
            );
        }
        Ok(remote_commitment == local_commitment)
    }

    /// Because the node can fetch L1 batch root hash from an external source using the tree data fetcher, there's no strict guarantee
    /// that L1 batch root hashes can necessarily be binary searched on their own (i.e., that there exists N such that root hashes of the first N batches match
    /// on the main node and this node, and root hashes of L1 batches N + 1, N + 2, ... diverge). The tree data fetcher makes a reasonable attempt
    /// to detect a reorg and to not persist root hashes for diverging L1 batches, but we don't trust this logic to work in all cases (yet?).
    ///
    /// Hence, we perform binary search both by L1 root hashes and the last L2 block hash in the batch; unlike L1 batches, L2 block hashes are *always* fully computed
    /// based only on data locally processed by the node. Additionally, an L2 block hash of the last block in a batch encompasses a reasonably large part of L1 batch contents.
    async fn root_hashes_and_contents_match(
        &self,
        l1_batch: L1BatchNumber,
    ) -> Result<bool, HashMatchError> {
        let root_hashes_match = self.root_hashes_match(l1_batch).await?;
        if !root_hashes_match {
            return Ok(false);
        }

        let mut storage = self.pool.connection().await?;
        let (_, last_l2_block_in_batch) = storage
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(l1_batch)
            .await?
            .with_context(|| format!("L1 batch #{l1_batch} does not have L2 blocks"))?;
        drop(storage);

        self.l2_block_hashes_match(last_l2_block_in_batch).await
    }

    /// Localizes a re-org: performs binary search to determine the last non-diverged L1 batch.
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
                match self
                    .root_hashes_and_contents_match(L1BatchNumber(number))
                    .await
                {
                    Err(HashMatchError::MissingData(_)) => Ok(true),
                    res => res,
                }
            },
        )
        .await
        .map(L1BatchNumber)
    }

    /// Runs this detector *once* checking whether there is a reorg.
    ///
    /// Only fatal errors are returned (incl. detected reorgs). Retriable errors are retried indefinitely accounting for a stop request.
    pub async fn run_once(
        &mut self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped<Error>> {
        self.run_inner(true, stop_receiver).await
    }

    /// Runs this detector continuously checking for a reorg until a fatal error occurs (including if a reorg is detected),
    /// or a stop request is received.
    pub async fn run(
        mut self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<Infallible, OrStopped<Error>> {
        self.event_handler.initialize();
        // `unwrap_err()` is safe: `run_inner()` can only return `Ok(())` if `stop_after_success` is set.
        let err = self.run_inner(false, stop_receiver).await.unwrap_err();
        if matches!(&err, OrStopped::Stopped) {
            self.event_handler.start_shutting_down();
            tracing::info!("Shutting down reorg detector");
        }
        Err(err)
    }

    async fn run_inner(
        &mut self,
        stop_after_success: bool,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped<Error>> {
        while !*stop_receiver.borrow_and_update() {
            let sleep_interval = match self.check_consistency().await {
                Err(Error::HashMatch(err)) => {
                    self.handle_hash_err(err).map_err(OrStopped::internal)?
                }
                Err(err) => return Err(err.into()),
                Ok(()) if stop_after_success => return Ok(()),
                Ok(()) => self.sleep_interval,
            };

            if tokio::time::timeout(sleep_interval, stop_receiver.changed())
                .await
                .is_ok()
            {
                // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
                // OTOH, an Ok(_) value always signals task termination.
                break;
            }
        }
        Err(OrStopped::Stopped)
    }

    /// Returns the sleep interval if the error is transient.
    fn handle_hash_err(&self, err: HashMatchError) -> Result<Duration, HashMatchError> {
        match err {
            HashMatchError::MissingData(MissingData::RootHash) => {
                tracing::debug!("Last L1 batch on the main node doesn't have a state root hash; waiting until it is computed");
                Ok(self.sleep_interval / 10)
            }
            err if err.is_retriable() => {
                tracing::warn!("Following transient error occurred: {err}");
                tracing::info!("Trying again after a delay");
                Ok(self.sleep_interval)
            }
            err => Err(err),
        }
    }

    /// Checks whether a reorg is present. Unlike [`Self::run_once()`], this method doesn't pinpoint the first diverged L1 batch;
    /// it just checks whether diverged batches / blocks exist in general.
    ///
    /// Internally retries transient errors.
    pub async fn check_reorg_presence(
        &mut self,
        mut stop_receiver: watch::Receiver<bool>,
        ignore_rpc_error: bool,
    ) -> Result<bool, OrStopped> {
        while !*stop_receiver.borrow_and_update() {
            let sleep_interval = match self.find_last_diverged_batch().await {
                Err(HashMatchError::Rpc(err)) => {
                    // If we have rpc error, we won't try to check the reorg presence.
                    // In the worst case, we will restart right after the main node recovers.
                    if ignore_rpc_error {
                        return Ok(false);
                    } else {
                        self.handle_hash_err(HashMatchError::Rpc(err))
                            .map_err(OrStopped::internal)?
                    }
                }
                Err(err) => self.handle_hash_err(err).map_err(OrStopped::internal)?,
                Ok(maybe_diverged_batch) => return Ok(maybe_diverged_batch.is_some()),
            };

            if tokio::time::timeout(sleep_interval, stop_receiver.changed())
                .await
                .is_ok()
            {
                return Err(OrStopped::Stopped);
            }
        }
        Ok(false)
    }
}

/// Fallible and async predicate for binary search.
#[async_trait]
trait BinarySearchPredicate: Send {
    type Error;

    async fn eval(&mut self, argument: u32) -> Result<bool, Self::Error>;
}

#[async_trait]
impl<F, Fut, E> BinarySearchPredicate for F
where
    F: Send + FnMut(u32) -> Fut,
    Fut: Send + Future<Output = Result<bool, E>>,
{
    type Error = E;

    async fn eval(&mut self, argument: u32) -> Result<bool, Self::Error> {
        self(argument).await
    }
}

/// Finds the greatest `u32` value for which `f` returns `true`.
async fn binary_search_with<P: BinarySearchPredicate>(
    mut left: u32,
    mut right: u32,
    mut predicate: P,
) -> Result<u32, P::Error> {
    while left + 1 < right {
        let middle = (left + right) / 2;
        if predicate.eval(middle).await? {
            left = middle;
        } else {
            right = middle;
        }
    }
    Ok(left)
}
