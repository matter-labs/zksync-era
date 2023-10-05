use tokio::sync::watch;

use std::sync::Arc;

use zksync_config::{
    configs::chain::{MempoolConfig, NetworkConfig, StateKeeperConfig},
    constants::MAX_TXS_IN_BLOCK,
    ContractsConfig, DBConfig,
};
use zksync_dal::ConnectionPool;
use zksync_types::L2ChainId;

mod batch_executor;
pub(crate) mod extractors;
pub(crate) mod io;
mod keeper;
mod mempool_actor;
pub(crate) mod seal_criteria;
#[cfg(test)]
mod tests;
pub(crate) mod types;
pub(crate) mod updates;

pub use self::{
    batch_executor::{L1BatchExecutorBuilder, MainBatchExecutorBuilder},
    keeper::ZkSyncStateKeeper,
    seal_criteria::SealManager,
};
pub(crate) use self::{io::MiniblockSealer, mempool_actor::MempoolFetcher, types::MempoolGuard};

use self::io::{MempoolIO, MiniblockSealerHandle};
use crate::l1_gas_price::L1GasPriceProvider;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_state_keeper<G>(
    contracts_config: &ContractsConfig,
    state_keeper_config: StateKeeperConfig,
    db_config: &DBConfig,
    network_config: &NetworkConfig,
    mempool_config: &MempoolConfig,
    pool: ConnectionPool,
    mempool: MempoolGuard,
    l1_gas_price_provider: Arc<G>,
    miniblock_sealer_handle: MiniblockSealerHandle,
    stop_receiver: watch::Receiver<bool>,
) -> ZkSyncStateKeeper
where
    G: L1GasPriceProvider + 'static + Send + Sync,
{
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
        state_keeper_config.enum_index_migration_chunks(),
    );

    let io = MempoolIO::new(
        mempool,
        miniblock_sealer_handle,
        l1_gas_price_provider,
        pool,
        &state_keeper_config,
        mempool_config.delay_interval(),
        contracts_config.l2_erc20_bridge_addr,
        state_keeper_config.validation_computational_gas_limit,
        L2ChainId(network_config.zksync_network_id),
    )
    .await;

    let sealer = SealManager::new(state_keeper_config);
    ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        sealer,
    )
}
