use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use rand::{thread_rng, Rng};
use zksync_dal::{pruning_dal::PruningInfo, Connection, Core, CoreDal, DalError};
use zksync_multivm::utils::get_eth_call_gas_limit;
use zksync_types::{
    api, fee_model::BatchFeeInput, L1BatchNumber, L2BlockNumber, ProtocolVersionId, U256,
};
use zksync_vm_executor::oneshot::{BlockInfo, ResolvedBlockInfo};

use self::vm_metrics::SandboxStage;
pub(crate) use self::{
    error::SandboxExecutionError,
    execute::{SandboxAction, SandboxExecutionOutput, SandboxExecutor},
    validate::ValidationError,
    vm_metrics::{SubmitTxStage, SANDBOX_METRICS},
};

// Note: keep the modules private, and instead re-export functions that make public interface.
mod error;
mod execute;
mod storage;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;
mod validate;
mod vm_metrics;

/// Permit to invoke VM code.
///
/// Any publicly-facing method that invokes VM is expected to accept a reference to this structure,
/// as a proof that the caller obtained a token from `VmConcurrencyLimiter`,
#[derive(Debug, Clone)]
pub struct VmPermit {
    _permit: Arc<tokio::sync::OwnedSemaphorePermit>,
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
            _permit: Arc::new(permit),
        })
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
pub struct BlockStartInfo {
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
        if let Some(pruned) = cached_pruning_info.last_soft_pruned {
            return Ok(pruned.l2_block + 1);
        }
        Ok(L2BlockNumber(0))
    }

    pub async fn first_l1_batch(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        let cached_pruning_info = self.get_pruning_info(storage).await?;
        if let Some(pruned) = cached_pruning_info.last_soft_pruned {
            return Ok(pruned.l1_batch + 1);
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
pub enum BlockArgsError {
    #[error("Block is not available, either it was pruned or the node was started from a snapshot created later than this block; first retained block is {0}")]
    Pruned(L2BlockNumber),
    #[error("Block is missing, but can appear in the future")]
    Missing,
    #[error("Database error")]
    Database(#[from] anyhow::Error),
}

/// Information about a block provided to VM.
#[derive(Debug, Clone)]
pub struct BlockArgs {
    inner: BlockInfo,
    resolved: ResolvedBlockInfo,
    block_id: api::BlockId,
}

impl BlockArgs {
    pub async fn pending(connection: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
        let inner = BlockInfo::pending(connection).await?;
        let resolved = inner.resolve(connection).await?;
        Ok(Self {
            inner,
            resolved,
            block_id: api::BlockId::Number(api::BlockNumber::Pending),
        })
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        self.resolved.protocol_version()
    }

    pub fn use_evm_emulator(&self) -> bool {
        self.resolved.use_evm_emulator()
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
            return Ok(Self::pending(connection).await?);
        }

        let resolved_block_number = connection
            .blocks_web3_dal()
            .resolve_block_id(block_id)
            .await
            .map_err(DalError::generalize)?;
        let Some(block_number) = resolved_block_number else {
            return Err(BlockArgsError::Missing);
        };

        let inner = BlockInfo::for_existing_block(connection, block_number).await?;
        Ok(Self {
            inner,
            resolved: inner.resolve(connection).await?,
            block_id,
        })
    }

    pub fn resolved_block_number(&self) -> L2BlockNumber {
        self.inner.block_number()
    }

    fn is_pending(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(api::BlockNumber::Pending)
        )
    }

    pub fn resolves_to_latest_sealed_l2_block(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(
                api::BlockNumber::Pending | api::BlockNumber::Latest | api::BlockNumber::Committed
            )
        )
    }

    pub async fn historical_fee_input(
        &self,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<BatchFeeInput> {
        self.inner.historical_fee_input(connection).await
    }

    /// Calculates the effective gas limit applying the gas cap if specified.
    /// Returns the minimum of protocol default and gas cap (if cap is set and > 0).
    pub fn calculate_effective_gas_limit(
        protocol_version: ProtocolVersionId,
        gas_cap: Option<u64>,
    ) -> u64 {
        let default_gas_limit = get_eth_call_gas_limit(protocol_version.into());

        // Apply gas cap if specified (0 means no cap)
        if let Some(cap) = gas_cap.filter(|&cap| cap > 0) {
            std::cmp::min(default_gas_limit, cap)
        } else {
            default_gas_limit
        }
    }

    pub async fn default_eth_call_gas(
        &self,
        connection: &mut Connection<'_, Core>,
        gas_cap: Option<u64>,
    ) -> anyhow::Result<U256> {
        let protocol_version = if self.is_pending() {
            connection.blocks_dal().pending_protocol_version().await?
        } else {
            let block_number = self.inner.block_number();
            connection
                .blocks_dal()
                .get_l2_block_header(block_number)
                .await?
                .with_context(|| format!("missing header for resolved block #{block_number}"))?
                .protocol_version
                .unwrap_or_else(ProtocolVersionId::last_potentially_undefined)
        };

        let effective_gas_limit = Self::calculate_effective_gas_limit(protocol_version, gas_cap);
        Ok(effective_gas_limit.into())
    }
}

#[cfg(test)]
mod gas_cap_tests {
    use super::BlockArgs;
    use zksync_types::ProtocolVersionId;

    #[test]
    fn test_gas_cap_logic() {
        // Get a sample protocol version
        let protocol_version = ProtocolVersionId::latest();

        // Test 1: No gas cap (should use protocol default)
        let result_no_cap = BlockArgs::calculate_effective_gas_limit(protocol_version, None);
        let expected_default = zksync_multivm::utils::get_eth_call_gas_limit(protocol_version.into());
        assert_eq!(
            result_no_cap, expected_default,
            "No gas cap should use protocol default"
        );

        // Test 2: Gas cap of 0 (should use protocol default)
        let result_zero_cap = BlockArgs::calculate_effective_gas_limit(protocol_version, Some(0));
        assert_eq!(
            result_zero_cap, expected_default,
            "Zero gas cap should use protocol default"
        );

        // Test 3: Gas cap larger than protocol default (should use protocol default)
        let large_gas_cap = expected_default + 1_000_000;
        let result_large_cap = BlockArgs::calculate_effective_gas_limit(protocol_version, Some(large_gas_cap));
        assert_eq!(
            result_large_cap, expected_default,
            "Large gas cap should not exceed protocol default"
        );

        // Test 4: Gas cap smaller than protocol default (should use gas cap)
        let small_gas_cap = 100_000u64;
        let result_small_cap = BlockArgs::calculate_effective_gas_limit(protocol_version, Some(small_gas_cap));
        assert_eq!(
            result_small_cap, small_gas_cap,
            "Small gas cap should limit the gas"
        );
        assert!(
            result_small_cap < expected_default,
            "Capped gas should be less than uncapped"
        );
    }
}
