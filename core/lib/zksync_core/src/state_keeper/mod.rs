use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::{
    configs::chain::{MempoolConfig, NetworkConfig, StateKeeperConfig},
    DBConfig,
};
use zksync_dal::{ConnectionPool, Core};

pub use self::{
    batch_executor::{main_executor::MainBatchExecutor, BatchExecutor},
    io::{
        mempool::MempoolIO, MiniblockSealerTask, OutputHandler, StateKeeperIO,
        StateKeeperOutputHandler, StateKeeperPersistence,
    },
    keeper::ZkSyncStateKeeper,
    mempool_actor::MempoolFetcher,
    seal_criteria::SequencerSealer,
    types::MempoolGuard,
};
use crate::fee_model::BatchFeeModelInputProvider;

mod batch_executor;
pub(crate) mod extractors;
pub(crate) mod io;
mod keeper;
mod mempool_actor;
pub(crate) mod metrics;
pub mod seal_criteria;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod types;
pub(crate) mod updates;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_state_keeper(
    state_keeper_config: StateKeeperConfig,
    db_config: &DBConfig,
    network_config: &NetworkConfig,
    mempool_config: &MempoolConfig,
    pool: ConnectionPool<Core>,
    mempool: MempoolGuard,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    output_handler: OutputHandler,
    stop_receiver: watch::Receiver<bool>,
) -> ZkSyncStateKeeper {
    let batch_executor_base = MainBatchExecutor::new(
        db_config.state_keeper_db_path.clone(),
        pool.clone(),
        state_keeper_config.save_call_traces,
        state_keeper_config.upload_witness_inputs_to_gcs,
        state_keeper_config.enum_index_migration_chunk_size(),
        false,
    );

    let io = MempoolIO::new(
        mempool,
        batch_fee_input_provider,
        pool,
        &state_keeper_config,
        mempool_config.delay_interval(),
        network_config.zksync_network_id,
    )
    .await
    .expect("Failed initializing main node I/O for state keeper");

    let sealer = SequencerSealer::new(state_keeper_config);
    ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        output_handler,
        Arc::new(sealer),
    )
}
