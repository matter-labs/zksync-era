mod metrics;

use std::{future::Future, sync::Arc, time::Duration};

use chrono::NaiveDateTime;
use tokio::sync::{watch, RwLock};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::H256;

use crate::{cache::sequential_cache::SequentialCache, mempool_cache::metrics::METRICS};

/// Used for `eth_newPendingTransactionFilter` requests on API servers
/// Stores all transactions accepted by the mempool and provides a way to query all that are newer than a given timestamp.
/// Updates the cache based on interval passed in the constructor
#[derive(Debug, Clone)]
pub struct MempoolCache(Arc<RwLock<SequentialCache<NaiveDateTime, H256>>>);

/// `INITIAL_LOOKBEHIND` is the period of time for which the cache is initially populated.
const INITIAL_LOOKBEHIND: Duration = Duration::from_secs(120);

impl MempoolCache {
    /// Initializes the mempool cache with the parameters provided.
    ///
    /// # Panics
    ///
    /// Panics if `dal` returns a non-ordered list of transactions.

    pub fn new(
        connection_pool: ConnectionPool<Core>,
        update_interval: Duration,
        capacity: usize,
        stop_receiver: watch::Receiver<bool>,
    ) -> (Self, impl Future<Output = anyhow::Result<()>>) {
        let result: Self = Self(Arc::new(RwLock::new(SequentialCache::new(
            "mempool", capacity,
        ))));
        let this = result.clone();
        let update_task = async move {
            loop {
                if *stop_receiver.borrow() {
                    tracing::debug!("Stopping mempool cache updates");
                    return Ok(());
                }

                // Get the timestamp that will be used as the lower bound for the next update
                // If cache is non-empty - this is the last tx time, otherwise it's `INITIAL_LOOKBEHIND` seconds ago
                let last_timestamp = this
                    .0
                    .read()
                    .await
                    .get_last_key()
                    .unwrap_or_else(|| chrono::Utc::now().naive_utc() - INITIAL_LOOKBEHIND);

                let latency = METRICS.db_poll_latency.start();
                let mut connection = connection_pool.connection_tagged("api").await?;

                let txs = connection
                    .transactions_web3_dal()
                    .get_pending_txs_hashes_after(last_timestamp, None)
                    .await?;

                latency.observe();
                METRICS.tx_batch_size.observe(txs.len());

                this.0
                    .write()
                    .await
                    .insert(txs)
                    .expect("DAL guarantees keys to be ordered");

                tokio::time::sleep(update_interval).await;
            }
        };

        (result, update_task)
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
