use futures::channel::mpsc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};
use tokio::{runtime::Runtime, time};
use zksync_dal::ConnectionPool;
use zksync_types::{api, MiniblockNumber};
use zksync_utils::panic_notify::ThreadPanicNotify;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct NetworkStats {
    pub last_sealed: MiniblockNumber,
    pub last_verified: MiniblockNumber,
    pub total_transactions: usize,
}

#[derive(Debug, Default, Clone)]
pub struct SharedNetworkStats(Arc<RwLock<NetworkStats>>);

impl SharedNetworkStats {
    pub async fn read(&self) -> NetworkStats {
        (*self.0.as_ref().read().await).clone()
    }

    pub fn start_updater_detached(
        self,
        panic_notify: mpsc::Sender<bool>,
        connection_pool: ConnectionPool,
        polling_interval: Duration,
        stop_receiver: watch::Receiver<bool>,
    ) {
        std::thread::Builder::new()
            .name("explorer-stats-updater".to_string())
            .spawn(move || {
                let _panic_sentinel = ThreadPanicNotify(panic_notify.clone());

                let runtime = Runtime::new().expect("Failed to create tokio runtime");

                let stats_update_task = async move {
                    let mut timer = time::interval(polling_interval);
                    loop {
                        if *stop_receiver.borrow() {
                            vlog::warn!(
                                "Stop signal received, explorer_stats_updater is shutting down"
                            );
                            break;
                        }

                        timer.tick().await;

                        let mut storage = connection_pool.access_storage().await;

                        let last_sealed = storage
                            .blocks_web3_dal()
                            .get_sealed_miniblock_number()
                            .unwrap();
                        let last_verified = storage
                            .blocks_web3_dal()
                            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Finalized))
                            .unwrap()
                            .unwrap_or(MiniblockNumber(0));
                        let prev_stats = self.read().await;
                        let new_transactions = storage
                            .explorer()
                            .transactions_dal()
                            .get_transactions_count_after(prev_stats.last_sealed)
                            .unwrap();

                        let stats = NetworkStats {
                            last_sealed,
                            last_verified,
                            total_transactions: prev_stats.total_transactions + new_transactions,
                        };

                        // save stats to state
                        *self.0.as_ref().write().await = stats;
                    }
                };
                runtime.block_on(stats_update_task);
            })
            .expect("Failed to start thread for network stats updating");
    }
}
