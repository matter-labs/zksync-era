use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::{
    configs::chain::{MempoolConfig, NetworkConfig, StateKeeperConfig},
    ContractsConfig, DBConfig,
};
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_system_constants::MAX_TXS_IN_BLOCK;

use self::io::MempoolIO;
pub use self::{
    batch_executor::{L1BatchExecutorBuilder, MainBatchExecutorBuilder},
    io::{MiniblockSealer, MiniblockSealerHandle},
    keeper::ZkSyncStateKeeper,
};
pub(crate) use self::{
    mempool_actor::MempoolFetcher, seal_criteria::SequencerSealer, types::MempoolGuard,
};
use crate::l1_gas_price::L1GasPriceProvider;

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
    contracts_config: &ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    db_config: &DBConfig,
    network_config: &NetworkConfig,
    mempool_config: &MempoolConfig,
    pool: ConnectionPool,
    mempool: MempoolGuard,
    l1_gas_price_provider: Arc<dyn L1GasPriceProvider>,
    miniblock_sealer_handle: MiniblockSealerHandle,
    object_store: Arc<dyn ObjectStore>,
    stop_receiver: watch::Receiver<bool>,
) -> ZkSyncStateKeeper {
    assert!(
        state_keeper_config.transaction_slots <= MAX_TXS_IN_BLOCK,
        "Configured transaction_slots ({}) must be lower than the bootloader constant MAX_TXS_IN_BLOCK={}",
        state_keeper_config.transaction_slots,
        MAX_TXS_IN_BLOCK
    );

    let batch_executor_base = MainBatchExecutorBuilder::new(
        db_config.state_keeper_db_path.clone(),
        pool.clone(),
        state_keeper_config.max_allowed_l2_tx_gas_limit.into(),
        state_keeper_config.save_call_traces,
        state_keeper_config.upload_witness_inputs_to_gcs,
        state_keeper_config.enum_index_migration_chunk_size(),
    );

    let io = MempoolIO::new(
        mempool,
        object_store,
        miniblock_sealer_handle,
        l1_gas_price_provider,
        pool,
        &state_keeper_config,
        mempool_config.delay_interval(),
        contracts_config.l2_erc20_bridge_addr,
        state_keeper_config.validation_computational_gas_limit,
        network_config.zksync_network_id,
    )
    .await;

    let sealer = SequencerSealer::new(state_keeper_config);
    ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        Box::new(sealer),
    )
}
