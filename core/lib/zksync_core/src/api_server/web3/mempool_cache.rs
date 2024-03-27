use std::{future::Future, sync::Arc, time::Duration};

use chrono::NaiveDateTime;
use tokio::sync::{watch, RwLock};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_state::SequentialCache;
use zksync_types::H256;

use super::metrics::MEMPOOL_CACHE_METRICS;

/// Used for `eth_newPendingTransactionFilter` requests on API servers
/// Stores all transactions accepted by the mempool and provides a way to query all that are newer than a given timestamp.
/// Updates the cache based on interval passed in the constructor
#[derive(Debug, Clone)]
pub(crate) struct MempoolCache(Arc<RwLock<SequentialCache<NaiveDateTime, H256>>>);

/// `INITIAL_LOOKBEHIND` is the period of time for which the cache is initially populated.
const INITIAL_LOOKBEHIND: Duration = Duration::from_secs(120);

impl MempoolCache {
    /// Initializes the mempool cache with the parameters provided.
    pub fn new(
        connection_pool: ConnectionPool<Core>,
        update_interval: Duration,
        capacity: usize,
        stop_receiver: watch::Receiver<bool>,
    ) -> (Self, impl Future<Output = anyhow::Result<()>>) {
        let cache = SequentialCache::new("mempool", capacity);
        let cache = Arc::new(RwLock::new(cache));
        let cache_for_task = cache.clone();
        let update_task = async move {
            loop {
                if *stop_receiver.borrow() {
                    tracing::debug!("Stopping mempool cache updates");
                    return Ok(());
                }

                // Get the timestamp that will be used as the lower bound for the next update
                // If cache is non-empty - this is the last tx time, otherwise it's `INITIAL_LOOKBEHIND` seconds ago
                let last_timestamp = cache_for_task
                    .read()
                    .await
                    .get_last_key()
                    .unwrap_or_else(|| chrono::Utc::now().naive_utc() - INITIAL_LOOKBEHIND);

                let latency = MEMPOOL_CACHE_METRICS.db_poll_latency.start();
                let mut connection = connection_pool.connection_tagged("api").await?;
                let txs = connection
                    .transactions_web3_dal()
                    .get_pending_txs_hashes_after(last_timestamp, None)
                    .await?;
                drop(connection);
                latency.observe();
                MEMPOOL_CACHE_METRICS.tx_batch_size.observe(txs.len());

                cache_for_task.write().await.insert(txs)?;
                tokio::time::sleep(update_interval).await;
            }
        };

        (Self(cache), update_task)
    }

    /// Returns all transaction hashes that are newer than the given timestamp.
    /// Does not include the transactions that are exactly at the given timestamp.
    pub async fn get_tx_hashes_after(
        &self,
        after: NaiveDateTime,
    ) -> Option<Vec<(NaiveDateTime, H256)>> {
        self.0.read().await.query(after)
    }
}
