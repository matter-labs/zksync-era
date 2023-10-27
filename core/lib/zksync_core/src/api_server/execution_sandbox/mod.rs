use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;

use zksync_dal::{ConnectionPool, SqlxError, StorageProcessor};
use zksync_state::{PostgresStorage, PostgresStorageCaches, ReadStorage, StorageView};
use zksync_system_constants::PUBLISH_BYTECODE_OVERHEAD;
use zksync_types::{api, AccountTreeId, L2ChainId, MiniblockNumber, U256};
use zksync_utils::bytecode::{compress_bytecode, hash_bytecode};

// Note: keep the modules private, and instead re-export functions that make public interface.
mod apply;
mod error;
mod execute;
mod tracers;
mod validate;
mod vm_metrics;

use self::vm_metrics::SandboxStage;
pub(super) use self::{
    error::SandboxExecutionError,
    execute::{execute_tx_eth_call, execute_tx_with_pending_state, TxExecutionArgs},
    tracers::ApiTracer,
    vm_metrics::{SubmitTxStage, SANDBOX_METRICS},
};
use super::tx_sender::MultiVMBaseSystemContracts;
use multivm::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata;

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

pub(super) fn adjust_l1_gas_price_for_tx(
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    tx_gas_per_pubdata_limit: U256,
) -> u64 {
    let (_, current_pubdata_price) =
        derive_base_fee_and_gas_per_pubdata(l1_gas_price, fair_l2_gas_price);
    if U256::from(current_pubdata_price) <= tx_gas_per_pubdata_limit {
        // The current pubdata price is small enough
        l1_gas_price
    } else {
        // gasPerPubdata = ceil(17 * l1gasprice / fair_l2_gas_price)
        // gasPerPubdata <= 17 * l1gasprice / fair_l2_gas_price + 1
        // fair_l2_gas_price(gasPerPubdata - 1) / 17 <= l1gasprice
        let l1_gas_price = U256::from(fair_l2_gas_price)
            * (tx_gas_per_pubdata_limit - U256::from(1u32))
            / U256::from(17);

        l1_gas_price.as_u64()
    }
}

async fn get_pending_state(
    connection: &mut StorageProcessor<'_>,
) -> (api::BlockId, MiniblockNumber) {
    let block_id = api::BlockId::Number(api::BlockNumber::Pending);
    let resolved_block_number = connection
        .blocks_web3_dal()
        .resolve_block_id(block_id)
        .await
        .unwrap()
        .expect("Pending block should be present");
    (block_id, resolved_block_number)
}

/// Returns the number of the pubdata that the transaction will spend on factory deps.
pub(super) async fn get_pubdata_for_factory_deps(
    _vm_permit: &VmPermit,
    connection_pool: &ConnectionPool,
    factory_deps: &[Vec<u8>],
    storage_caches: PostgresStorageCaches,
) -> u32 {
    if factory_deps.is_empty() {
        return 0; // Shortcut for the common case allowing to not acquire DB connections etc.
    }

    let mut connection = connection_pool.access_storage_tagged("api").await.unwrap();
    let (_, block_number) = get_pending_state(&mut connection).await;
    drop(connection);

    let rt_handle = Handle::current();
    let connection_pool = connection_pool.clone();
    let factory_deps = factory_deps.to_vec();
    tokio::task::spawn_blocking(move || {
        let connection = rt_handle
            .block_on(connection_pool.access_storage_tagged("api"))
            .unwrap();
        let storage = PostgresStorage::new(rt_handle, connection, block_number, false)
            .with_caches(storage_caches);
        let mut storage_view = StorageView::new(storage);

        let effective_lengths = factory_deps.iter().map(|bytecode| {
            if storage_view.is_bytecode_known(&hash_bytecode(bytecode)) {
                return 0;
            }

            let length = if let Ok(compressed) = compress_bytecode(bytecode) {
                compressed.len()
            } else {
                bytecode.len()
            };
            length as u32 + PUBLISH_BYTECODE_OVERHEAD
        });
        effective_lengths.sum()
    })
    .await
    .unwrap()
}

/// Arguments for VM execution not specific to a particular transaction.
#[derive(Debug, Clone)]
pub(crate) struct TxSharedArgs {
    pub operator_account: AccountTreeId,
    pub l1_gas_price: u64,
    pub fair_l2_gas_price: u64,
    pub base_system_contracts: MultiVMBaseSystemContracts,
    pub caches: PostgresStorageCaches,
    pub validation_computational_gas_limit: u32,
    pub chain_id: L2ChainId,
}

/// Information about a block provided to VM.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockArgs {
    block_id: api::BlockId,
    resolved_block_number: MiniblockNumber,
    l1_batch_timestamp_s: Option<u64>,
}

impl BlockArgs {
    async fn pending(connection: &mut StorageProcessor<'_>) -> Self {
        let (block_id, resolved_block_number) = get_pending_state(connection).await;
        Self {
            block_id,
            resolved_block_number,
            l1_batch_timestamp_s: None,
        }
    }

    /// Loads block information from DB.
    pub async fn new(
        connection: &mut StorageProcessor<'_>,
        block_id: api::BlockId,
    ) -> Result<Option<Self>, SqlxError> {
        if block_id == api::BlockId::Number(api::BlockNumber::Pending) {
            return Ok(Some(BlockArgs::pending(connection).await));
        }

        let resolved_block_number = connection
            .blocks_web3_dal()
            .resolve_block_id(block_id)
            .await?;
        let Some(resolved_block_number) = resolved_block_number else {
            return Ok(None);
        };

        let l1_batch_number = connection
            .storage_web3_dal()
            .resolve_l1_batch_number_of_miniblock(resolved_block_number)
            .await?
            .expected_l1_batch();
        let l1_batch_timestamp_s = connection
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(l1_batch_number)
            .await?;
        assert!(
            l1_batch_timestamp_s.is_some(),
            "Missing batch timestamp for non-pending block"
        );
        Ok(Some(Self {
            block_id,
            resolved_block_number,
            l1_batch_timestamp_s,
        }))
    }

    pub fn resolved_block_number(&self) -> MiniblockNumber {
        self.resolved_block_number
    }

    pub fn resolves_to_latest_sealed_miniblock(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(
                api::BlockNumber::Pending | api::BlockNumber::Latest | api::BlockNumber::Committed
            )
        )
    }
}
