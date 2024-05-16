use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use rand::{thread_rng, Rng};
use tokio::runtime::Handle;
use zksync_dal::{pruning_dal::PruningInfo, Connection, Core, CoreDal, DalError};
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    api, fee_model::BatchFeeInput, AccountTreeId, Address, L1BatchNumber, L2BlockNumber, L2ChainId,
};

use self::vm_metrics::SandboxStage;
pub(super) use self::{
    error::SandboxExecutionError,
    execute::{TransactionExecutor, TxExecutionArgs},
    tracers::ApiTracer,
    validate::ValidationError,
    vm_metrics::{SubmitTxStage, SANDBOX_METRICS},
};
use super::tx_sender::MultiVMBaseSystemContracts;

// Note: keep the modules private, and instead re-export functions that make public interface.
mod apply;
mod error;
mod execute;
pub mod testonly;
#[cfg(test)]
mod tests;
mod tracers;
mod validate;
mod vm_metrics;

/// Permit to invoke VM code.
///
/// Any publicly-facing method that invokes VM is expected to accept a reference to this structure,
/// as a proof that the caller obtained a token from `VmConcurrencyLimiter`,
#[derive(Debug, Clone)]
pub struct VmPermit {
    /// A handle to the runtime that is used to query the VM storage.
    rt_handle: Handle,
    _permit: Arc<tokio::sync::OwnedSemaphorePermit>,
}

impl VmPermit {
    fn rt_handle(&self) -> &Handle {
        &self.rt_handle
    }
}

/// Barrier-like synchronization primitive allowing to close a [`VmConcurrencyLimiter`] it's attached to
/// so that it doesn't issue new permits, and to wait for all permits to drop.
#[derive(Debug, Clone)]
pub struct VmConcurrencyBarrier {
    limiter: Arc<tokio::sync::Semaphore>,
    max_concurrency: usize,
}

impl VmConcurrencyBarrier {
    /// Shuts down the related VM concurrency limiter so that it won't issue new permits.
    pub fn close(&self) {
        self.limiter.close();
        tracing::info!("VM concurrency limiter closed");
    }

    /// Waits until all permits issued by the VM concurrency limiter are dropped.
    pub async fn wait_until_stopped(self) {
        const POLL_INTERVAL: Duration = Duration::from_millis(50);

        assert!(
            self.limiter.is_closed(),
            "Cannot wait on non-closed VM concurrency limiter"
        );

        loop {
            let current_permits = self.limiter.available_permits();
            tracing::debug!(
                "Waiting until all VM permits are dropped; currently remaining: {} / {}",
                self.max_concurrency - current_permits,
                self.max_concurrency
            );
            if current_permits == self.max_concurrency {
                return;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

/// Synchronization primitive that limits the number of concurrent VM executions.
/// This is required to prevent the server from being overloaded with the VM calls.
///
/// This structure is expected to be used in every method that executes VM code, on a topmost
/// level (i.e. before any async calls are made or VM is instantiated),
///
/// Note that the actual limit on the number of VMs is a minimum of the limit in this structure,
/// *and* the size of the blocking tokio threadpool. So, even if the limit is set to 1024, but
/// tokio is configured to have no more than 512 blocking threads, the actual limit will be 512.
#[derive(Debug)]
pub struct VmConcurrencyLimiter {
    /// Semaphore that limits the number of concurrent VM executions.
    limiter: Arc<tokio::sync::Semaphore>,
    rt_handle: Handle,
}

impl VmConcurrencyLimiter {
    /// Creates a limiter together with a barrier allowing to control its shutdown.
    pub fn new(max_concurrency: usize) -> (Self, VmConcurrencyBarrier) {
        tracing::info!(
            "Initializing the VM concurrency limiter with max concurrency {max_concurrency}"
        );
        let limiter = Arc::new(tokio::sync::Semaphore::new(max_concurrency));

        let this = Self {
            limiter: Arc::clone(&limiter),
            rt_handle: Handle::current(),
        };
        let barrier = VmConcurrencyBarrier {
            limiter,
            max_concurrency,
        };
        (this, barrier)
    }

    /// Waits until there is a free slot in the concurrency limiter.
    /// Returns a permit that should be dropped when the VM execution is finished.
    pub async fn acquire(&self) -> Option<VmPermit> {
        let available_permits = self.limiter.available_permits();
        SANDBOX_METRICS
            .sandbox_execution_permits
            .observe(available_permits);

        let latency = SANDBOX_METRICS.sandbox[&SandboxStage::VmConcurrencyLimiterAcquire].start();
        let permit = Arc::clone(&self.limiter).acquire_owned().await.ok()?;
        let elapsed = latency.observe();
        // We don't want to emit too many logs.
        if elapsed > Duration::from_millis(10) {
            tracing::debug!(
                "Permit is obtained. Available permits: {available_permits}. Took {elapsed:?}"
            );
        }

        Some(VmPermit {
            rt_handle: self.rt_handle.clone(),
            _permit: Arc::new(permit),
        })
    }
}

async fn get_pending_state(
    connection: &mut Connection<'_, Core>,
) -> anyhow::Result<(api::BlockId, L2BlockNumber)> {
    let block_id = api::BlockId::Number(api::BlockNumber::Pending);
    let resolved_block_number = connection
        .blocks_web3_dal()
        .resolve_block_id(block_id)
        .await
        .map_err(DalError::generalize)?
        .context("pending block should always be present in Postgres")?;
    Ok((block_id, resolved_block_number))
}

/// Arguments for VM execution not specific to a particular transaction.
#[derive(Debug, Clone)]
pub(crate) struct TxSharedArgs {
    pub operator_account: AccountTreeId,
    pub fee_input: BatchFeeInput,
    pub base_system_contracts: MultiVMBaseSystemContracts,
    pub caches: PostgresStorageCaches,
    pub validation_computational_gas_limit: u32,
    pub chain_id: L2ChainId,
    pub whitelisted_tokens_for_aa: Vec<Address>,
}

impl TxSharedArgs {
    #[cfg(test)]
    pub fn mock(base_system_contracts: MultiVMBaseSystemContracts) -> Self {
        Self {
            operator_account: AccountTreeId::default(),
            fee_input: BatchFeeInput::l1_pegged(55, 555),
            base_system_contracts,
            caches: PostgresStorageCaches::new(1, 1),
            validation_computational_gas_limit: u32::MAX,
            chain_id: L2ChainId::default(),
            whitelisted_tokens_for_aa: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BlockStartInfoInner {
    info: PruningInfo,
    cached_at: Instant,
}

impl BlockStartInfoInner {
    // We make max age a bit random so that all threads don't start refreshing cache at the same time
    const MAX_RANDOM_DELAY: Duration = Duration::from_millis(100);

    fn is_expired(&self, now: Instant, max_cache_age: Duration) -> bool {
        if let Some(expired_for) = (now - self.cached_at).checked_sub(max_cache_age) {
            if expired_for > Self::MAX_RANDOM_DELAY {
                return true; // The cache is definitely expired, regardless of the randomness below
            }
            // Minimize access to RNG, which could be mildly costly
            expired_for > thread_rng().gen_range(Duration::ZERO..=Self::MAX_RANDOM_DELAY)
        } else {
            false // `now` is close to `self.cached_at`; the cache isn't expired
        }
    }
}

/// Information about first L1 batch / L2 block in the node storage.
#[derive(Debug, Clone)]
pub(crate) struct BlockStartInfo {
    cached_pruning_info: Arc<RwLock<BlockStartInfoInner>>,
    max_cache_age: Duration,
}

impl BlockStartInfo {
    pub async fn new(
        storage: &mut Connection<'_, Core>,
        max_cache_age: Duration,
    ) -> anyhow::Result<Self> {
        let info = storage.pruning_dal().get_pruning_info().await?;
        Ok(Self {
            cached_pruning_info: Arc::new(RwLock::new(BlockStartInfoInner {
                info,
                cached_at: Instant::now(),
            })),
            max_cache_age,
        })
    }

    fn copy_inner(&self) -> BlockStartInfoInner {
        *self
            .cached_pruning_info
            .read()
            .expect("BlockStartInfo is poisoned")
    }

    async fn update_cache(
        &self,
        storage: &mut Connection<'_, Core>,
        now: Instant,
    ) -> anyhow::Result<PruningInfo> {
        let info = storage.pruning_dal().get_pruning_info().await?;

        let mut new_cached_pruning_info = self
            .cached_pruning_info
            .write()
            .map_err(|_| anyhow::anyhow!("BlockStartInfo is poisoned"))?;
        Ok(if new_cached_pruning_info.cached_at < now {
            *new_cached_pruning_info = BlockStartInfoInner {
                info,
                cached_at: now,
            };
            info
        } else {
            // Got a newer cache already; no need to update it again.
            new_cached_pruning_info.info
        })
    }

    async fn get_pruning_info(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<PruningInfo> {
        let inner = self.copy_inner();
        let now = Instant::now();
        if inner.is_expired(now, self.max_cache_age) {
            // Multiple threads may execute this query if we're very unlucky
            self.update_cache(storage, now).await
        } else {
            Ok(inner.info)
        }
    }

    pub async fn first_l2_block(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L2BlockNumber> {
        let cached_pruning_info = self.get_pruning_info(storage).await?;
        let last_block = cached_pruning_info.last_soft_pruned_l2_block;
        if let Some(L2BlockNumber(last_block)) = last_block {
            return Ok(L2BlockNumber(last_block + 1));
        }
        Ok(L2BlockNumber(0))
    }

    pub async fn first_l1_batch(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        let cached_pruning_info = self.get_pruning_info(storage).await?;
        let last_batch = cached_pruning_info.last_soft_pruned_l1_batch;
        if let Some(L1BatchNumber(last_block)) = last_batch {
            return Ok(L1BatchNumber(last_block + 1));
        }
        Ok(L1BatchNumber(0))
    }

    /// Checks whether a block with the specified ID is pruned and returns an error if it is.
    /// The `Err` variant wraps the first non-pruned L2 block.
    pub async fn ensure_not_pruned_block(
        &self,
        block: api::BlockId,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), BlockArgsError> {
        let first_l2_block = self
            .first_l2_block(storage)
            .await
            .map_err(BlockArgsError::Database)?;
        match block {
            api::BlockId::Number(api::BlockNumber::Number(number))
                if number < first_l2_block.0.into() =>
            {
                Err(BlockArgsError::Pruned(first_l2_block))
            }
            api::BlockId::Number(api::BlockNumber::Earliest)
                if first_l2_block > L2BlockNumber(0) =>
            {
                Err(BlockArgsError::Pruned(first_l2_block))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BlockArgsError {
    #[error("Block is pruned; first retained block is {0}")]
    Pruned(L2BlockNumber),
    #[error("Block is missing, but can appear in the future")]
    Missing,
    #[error("Database error")]
    Database(#[from] anyhow::Error),
}

/// Information about a block provided to VM.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockArgs {
    block_id: api::BlockId,
    resolved_block_number: L2BlockNumber,
    l1_batch_timestamp_s: Option<u64>,
}

impl BlockArgs {
    pub(crate) async fn pending(connection: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
        let (block_id, resolved_block_number) = get_pending_state(connection).await?;
        Ok(Self {
            block_id,
            resolved_block_number,
            l1_batch_timestamp_s: None,
        })
    }

    /// Loads block information from DB.
    pub async fn new(
        connection: &mut Connection<'_, Core>,
        block_id: api::BlockId,
        start_info: &BlockStartInfo,
    ) -> Result<Self, BlockArgsError> {
        // We need to check that `block_id` is present in Postgres or can be present in the future
        // (i.e., it does not refer to a pruned block). If called for a pruned block, the returned value
        // (specifically, `l1_batch_timestamp_s`) will be nonsensical.
        start_info
            .ensure_not_pruned_block(block_id, connection)
            .await?;

        if block_id == api::BlockId::Number(api::BlockNumber::Pending) {
            return Ok(BlockArgs::pending(connection).await?);
        }

        let resolved_block_number = connection
            .blocks_web3_dal()
            .resolve_block_id(block_id)
            .await
            .map_err(DalError::generalize)?;
        let Some(resolved_block_number) = resolved_block_number else {
            return Err(BlockArgsError::Missing);
        };

        let l1_batch = connection
            .storage_web3_dal()
            .resolve_l1_batch_number_of_l2_block(resolved_block_number)
            .await
            .with_context(|| {
                format!("failed resolving L1 batch number of L2 block #{resolved_block_number}")
            })?;
        let l1_batch_timestamp = connection
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&l1_batch)
            .await
            .map_err(DalError::generalize)?
            .context("missing timestamp for non-pending block")?;
        Ok(Self {
            block_id,
            resolved_block_number,
            l1_batch_timestamp_s: Some(l1_batch_timestamp),
        })
    }

    pub fn resolved_block_number(&self) -> L2BlockNumber {
        self.resolved_block_number
    }

    pub fn resolves_to_latest_sealed_l2_block(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(
                api::BlockNumber::Pending | api::BlockNumber::Latest | api::BlockNumber::Committed
            )
        )
    }
}
