use super::types::MempoolGuard;
use crate::GasAdjuster;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::watch;
use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_eth_client::clients::http_client::EthereumClient;

#[derive(Debug)]
pub struct MempoolFetcher {
    mempool: MempoolGuard,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
    sync_interval: Duration,
    sync_batch_size: usize,
}

impl MempoolFetcher {
    pub fn new(
        mempool: MempoolGuard,
        gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
        config: &ZkSyncConfig,
    ) -> Self {
        Self {
            mempool,
            gas_adjuster,
            sync_interval: config.chain.mempool.sync_interval(),
            sync_batch_size: config.chain.mempool.sync_batch_size,
        }
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        remove_stuck_txs: bool,
        stuck_tx_timeout: Duration,
        stop_receiver: watch::Receiver<bool>,
    ) {
        {
            let mut storage = pool.access_storage().await;
            if remove_stuck_txs {
                let removed_txs = storage
                    .transactions_dal()
                    .remove_stuck_txs(stuck_tx_timeout);
                vlog::info!("Number of stuck txs was removed: {}", removed_txs);
            }
            storage.transactions_dal().reset_mempool();
        }

        loop {
            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, mempool is shutting down");
                break;
            }
            let started_at = Instant::now();
            let mut storage = pool.access_storage().await;
            let mempool_info = self.mempool.get_mempool_info();
            let l2_tx_filter = self.gas_adjuster.l2_tx_filter();

            let (transactions, nonces) = storage.transactions_dal().sync_mempool(
                mempool_info.stashed_accounts,
                mempool_info.purged_accounts,
                l2_tx_filter.gas_per_pubdata,
                l2_tx_filter.fee_per_gas,
                self.sync_batch_size,
            );
            let all_transactions_loaded = transactions.len() < self.sync_batch_size;
            self.mempool.insert(transactions, nonces);
            metrics::histogram!("server.state_keeper.mempool_sync", started_at.elapsed());
            if all_transactions_loaded {
                tokio::time::sleep(self.sync_interval).await;
            }
        }
    }
}
