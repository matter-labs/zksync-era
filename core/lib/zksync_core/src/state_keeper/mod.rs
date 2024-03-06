use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::{
    configs::chain::{MempoolConfig, NetworkConfig, StateKeeperConfig},
    ContractsConfig, DBConfig,
};
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;

pub use self::{
    batch_executor::{main_executor::MainBatchExecutor, BatchExecutor},
    io::{mempool::MempoolIO, MiniblockSealer, MiniblockSealerHandle, StateKeeperIO},
    keeper::ZkSyncStateKeeper,
    mempool_actor::MempoolFetcher,
    seal_criteria::SequencerSealer,
    state_keeper_storage::StateKeeperStorage,
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
mod state_keeper_storage;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod types;
pub(crate) mod updates;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_state_keeper(
    contracts_config: &ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    db_config: &DBConfig,
    network_config: &NetworkConfig,
    mempool_config: &MempoolConfig,
    pool: ConnectionPool,
    mempool: MempoolGuard,
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    miniblock_sealer_handle: MiniblockSealerHandle,
    object_store: Arc<dyn ObjectStore>,
    stop_receiver: watch::Receiver<bool>,
) -> ZkSyncStateKeeper {
    let state_keeper_storage = StateKeeperStorage::async_rocksdb_cache(
        pool.clone(),
        db_config.state_keeper_db_path.clone(),
        state_keeper_config.enum_index_migration_chunk_size(),
    );
    let batch_executor_base = MainBatchExecutor::new(
        state_keeper_storage,
        state_keeper_config.max_allowed_l2_tx_gas_limit.into(),
        state_keeper_config.save_call_traces,
        state_keeper_config.upload_witness_inputs_to_gcs,
        false,
    );

    let io = MempoolIO::new(
        mempool,
        object_store,
        miniblock_sealer_handle,
        batch_fee_input_provider,
        pool,
        &state_keeper_config,
        mempool_config.delay_interval(),
        contracts_config.l2_erc20_bridge_addr,
        state_keeper_config.validation_computational_gas_limit,
        network_config.zksync_network_id,
    )
    .await
    .expect("Failed initializing main node I/O for state keeper");

    let sealer = SequencerSealer::new(state_keeper_config);
    ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        Arc::new(sealer),
    )
}
