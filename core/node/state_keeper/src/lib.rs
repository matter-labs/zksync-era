use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::{
    chain::{MempoolConfig, StateKeeperConfig},
    wallets,
};
use zksync_dal::{ConnectionPool, Core};
pub use zksync_multivm::FastVmMode;
use zksync_node_fee_model::BatchFeeModelInputProvider;
use zksync_types::L2ChainId;

pub use self::{
    batch_executor::{
        main_executor::MainBatchExecutor, BatchExecutor, BatchExecutorHandle, TxExecutionResult,
    },
    io::{
        mempool::MempoolIO, L2BlockParams, L2BlockSealerTask, OutputHandler, StateKeeperIO,
        StateKeeperOutputHandler, StateKeeperPersistence, TreeWritesPersistence,
    },
    keeper::ZkSyncStateKeeper,
    mempool_actor::MempoolFetcher,
    seal_criteria::SequencerSealer,
    state_keeper_storage::AsyncRocksdbCache,
    types::{ExecutionMetricsForCriteria, MempoolGuard},
    updates::UpdatesManager,
};

mod batch_executor;
pub mod io;
mod keeper;
mod mempool_actor;
pub mod metrics;
pub mod seal_criteria;
mod state_keeper_storage;
pub mod testonly;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod types;
pub mod updates;
pub(crate) mod utils;

#[allow(clippy::too_many_arguments)]
pub async fn create_state_keeper(
    state_keeper_config: StateKeeperConfig,
    wallets: wallets::StateKeeper,
    async_cache: AsyncRocksdbCache,
    l2chain_id: L2ChainId,
    mempool_config: &MempoolConfig,
    pool: ConnectionPool<Core>,
    mempool: MempoolGuard,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    output_handler: OutputHandler,
    stop_receiver: watch::Receiver<bool>,
) -> ZkSyncStateKeeper {
    let batch_executor_base = MainBatchExecutor::new(state_keeper_config.save_call_traces, false);

    let io = MempoolIO::new(
        mempool,
        batch_fee_input_provider,
        pool,
        &state_keeper_config,
        wallets.fee_account.address(),
        mempool_config.delay_interval(),
        l2chain_id,
    )
    .expect("Failed initializing main node I/O for state keeper");

    let sealer = SequencerSealer::new(state_keeper_config);

    ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        output_handler,
        Arc::new(sealer),
        Arc::new(async_cache),
    )
}
