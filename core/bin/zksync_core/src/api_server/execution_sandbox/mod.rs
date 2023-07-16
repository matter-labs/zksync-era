use std::time::{Duration, Instant};

use tokio::runtime::{Handle, Runtime};
use vm::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata;
use zksync_config::constants::PUBLISH_BYTECODE_OVERHEAD;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, SqlxError, StorageProcessor};
use zksync_state::{FactoryDepsCache, PostgresStorage, ReadStorage, StorageView};
use zksync_types::{api, AccountTreeId, MiniblockNumber, U256};
use zksync_utils::bytecode::{compress_bytecode, hash_bytecode};

// Note: keep the modules private, and instead re-export functions that make public interface.
mod apply;
mod error;
mod execute;
mod validate;
mod vm_metrics;

pub(super) use self::{
    error::SandboxExecutionError,
    execute::{execute_tx_eth_call, execute_tx_with_pending_state, TxExecutionArgs},
};

/// Permit to invoke VM code.
/// Any publicly-facing method that invokes VM is expected to accept a reference to this structure,
/// as a proof that the caller obtained a token from `VmConcurrencyLimiter`,
#[derive(Debug)]
pub struct VmPermit<'a> {
    _permit: tokio::sync::SemaphorePermit<'a>,
    /// A handle to the runtime that is used to query the VM storage.
    rt_handle: Handle,
}

impl<'a> VmPermit<'a> {
    fn rt_handle(&self) -> Handle {
        self.rt_handle.clone()
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
    limiter: tokio::sync::Semaphore,
    /// A dedicated runtime used to query the VM storage in the API.
    vm_runtime: RuntimeAccess,
}

/// Either a dedicated runtime, or a handle to the externally creatd runtime.
#[derive(Debug)]
enum RuntimeAccess {
    Owned(Runtime),
    Handle(Handle),
}

impl RuntimeAccess {
    fn handle(&self) -> Handle {
        match self {
            RuntimeAccess::Owned(rt) => rt.handle().clone(),
            RuntimeAccess::Handle(handle) => handle.clone(),
        }
    }
}

impl VmConcurrencyLimiter {
    pub fn new(max_concurrency: Option<usize>) -> Self {
        if let Some(max_concurrency) = max_concurrency {
            vlog::info!("Initializing the VM concurrency limiter with a separate runtime. Max concurrency: {:?}", max_concurrency);
            let vm_runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to initialize VM runtime");
            Self {
                limiter: tokio::sync::Semaphore::new(max_concurrency),
                vm_runtime: RuntimeAccess::Owned(vm_runtime),
            }
        } else {
            // Default concurrency is chosen to be beyond the number of connections in the pool /
            // amount of blocking threads in the tokio threadpool.
            // The real "concurrency limiter" will be represented by the lesser of these values.
            const DEFAULT_CONCURRENCY_LIMIT: usize = 2048;
            vlog::info!("Initializing the VM concurrency limiter with the default runtime");
            Self {
                limiter: tokio::sync::Semaphore::new(DEFAULT_CONCURRENCY_LIMIT),
                vm_runtime: RuntimeAccess::Handle(tokio::runtime::Handle::current()),
            }
        }
    }

    /// Waits until there is a free slot in the concurrency limiter.
    /// Returns a permit that should be dropped when the VM execution is finished.
    pub async fn acquire(&self) -> VmPermit<'_> {
        let available_permits = self.limiter.available_permits();
        metrics::histogram!(
            "api.web3.sandbox.semaphore.permits",
            available_permits as f64
        );

        let start = Instant::now();
        let permit = self
            .limiter
            .acquire()
            .await
            .expect("Semaphore is never closed");
        let elapsed = start.elapsed();
        // We don't want to emit too many logs.
        if elapsed > Duration::from_millis(10) {
            vlog::debug!(
                "Permit is obtained. Available permits: {available_permits}. Took {elapsed:?}"
            );
        }
        metrics::histogram!("api.web3.sandbox", elapsed, "stage" => "vm_concurrency_limiter_acquire");
        VmPermit {
            _permit: permit,
            rt_handle: self.vm_runtime.handle(),
        }
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
    connection_pool: &ConnectionPool,
    factory_deps: &[Vec<u8>],
    factory_deps_cache: FactoryDepsCache,
) -> u32 {
    if factory_deps.is_empty() {
        return 0; // Shortcut for the common case allowing to not acquire DB connections etc.
    }

    let mut connection = connection_pool.access_storage_tagged("api").await;
    let (_, block_number) = get_pending_state(&mut connection).await;
    drop(connection);

    let rt_handle = Handle::current();
    let connection_pool = connection_pool.clone();
    let factory_deps = factory_deps.to_vec();
    tokio::task::spawn_blocking(move || {
        let connection = rt_handle.block_on(connection_pool.access_storage_tagged("api"));
        let storage = PostgresStorage::new(rt_handle, connection, block_number, false)
            .with_factory_deps_cache(factory_deps_cache);
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
    pub base_system_contracts: BaseSystemContracts,
    pub factory_deps_cache: FactoryDepsCache,
}

/// Information about a block provided to VM.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockArgs {
    block_id: api::BlockId,
    resolved_block_number: MiniblockNumber,
    block_timestamp_s: Option<u64>,
}

impl BlockArgs {
    async fn pending(connection: &mut StorageProcessor<'_>) -> Self {
        let (block_id, resolved_block_number) = get_pending_state(connection).await;
        Self {
            block_id,
            resolved_block_number,
            block_timestamp_s: None,
        }
    }

    /// Loads block information from DB.
    pub async fn new(
        connection: &mut StorageProcessor<'_>,
        block_id: api::BlockId,
    ) -> Result<Option<Self>, SqlxError> {
        let resolved_block_number = connection
            .blocks_web3_dal()
            .resolve_block_id(block_id)
            .await?;
        let Some(resolved_block_number) = resolved_block_number else { return Ok(None) };

        let block_timestamp_s = connection
            .blocks_web3_dal()
            .get_block_timestamp(resolved_block_number)
            .await?;
        Ok(Some(Self {
            block_id,
            resolved_block_number,
            block_timestamp_s,
        }))
    }
}
