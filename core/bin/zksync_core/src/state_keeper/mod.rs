use std::sync::Arc;

use tokio::sync::watch::Receiver;

use zksync_config::constants::MAX_TXS_IN_BLOCK;
use zksync_config::ZkSyncConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::ConnectionPool;
use zksync_eth_client::EthInterface;

use self::batch_executor::MainBatchExecutorBuilder;
use self::io::MempoolIO;
use crate::gas_adjuster::GasAdjuster;
use crate::state_keeper::seal_criteria::SealManager;

pub use self::{keeper::ZkSyncStateKeeper, types::MempoolGuard};

pub(crate) mod batch_executor;
mod extractors;
pub(crate) mod io;
mod keeper;
pub(crate) mod mempool_actor;
pub mod seal_criteria;
#[cfg(test)]
mod tests;
pub(crate) mod types;
pub(crate) mod updates;

pub(crate) fn start_state_keeper<E>(
    config: &ZkSyncConfig,
    pool: &ConnectionPool,
    mempool: MempoolGuard,
    gas_adjuster: Arc<GasAdjuster<E>>,
    stop_receiver: Receiver<bool>,
) -> ZkSyncStateKeeper
where
    E: EthInterface + 'static + std::fmt::Debug + Send + Sync,
{
    assert!(
        config.chain.state_keeper.transaction_slots <= MAX_TXS_IN_BLOCK,
        "Configured transaction_slots must be lower than the bootloader constant MAX_TXS_IN_BLOCK"
    );

    let batch_executor_base = MainBatchExecutorBuilder::new(
        config.db.state_keeper_db_path.clone(),
        pool.clone(),
        config.chain.state_keeper.reexecute_each_tx,
        config.chain.state_keeper.max_allowed_l2_tx_gas_limit.into(),
        config.chain.state_keeper.validation_computational_gas_limit,
    );
    let io = MempoolIO::new(
        mempool,
        pool.clone(),
        config.chain.state_keeper.fee_account_addr,
        config.chain.state_keeper.fair_l2_gas_price,
        config.chain.operations_manager.delay_interval(),
        gas_adjuster,
        BaseSystemContractsHashes {
            bootloader: config.chain.state_keeper.bootloader_hash,
            default_aa: config.chain.state_keeper.default_aa_hash,
        },
    );

    let sealer = SealManager::new(config.chain.state_keeper.clone());
    ZkSyncStateKeeper::new(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
        sealer,
    )
}
