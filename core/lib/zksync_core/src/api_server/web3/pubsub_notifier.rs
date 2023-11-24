use anyhow::Context as _;
use jsonrpc_pubsub::typed;
use tokio::{
    sync::watch,
    time::{interval, Duration},
};

use zksync_dal::ConnectionPool;
use zksync_types::{MiniblockNumber, H256};
use zksync_web3_decl::types::{BlockHeader, Log, PubSubFilter, PubSubResult};

use super::{
    metrics::{SubscriptionType, PUB_SUB_METRICS},
    namespaces::SubscriptionMap,
};

/// Manager of notifications for a certain type of subscriptions.
#[derive(Debug)]
pub(super) struct PubSubNotifier<V> {
    subscribers: SubscriptionMap<V>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
}

impl<V: Clone> PubSubNotifier<V> {
    pub fn new(
        subscribers: SubscriptionMap<V>,
        connection_pool: ConnectionPool,
        polling_interval: Duration,
    ) -> Self {
        Self {
            subscribers,
            connection_pool,
            polling_interval,
        }
    }

    async fn sealed_miniblock_number(&self) -> anyhow::Result<MiniblockNumber> {
        self.connection_pool
            .access_storage_tagged("api")
            .await
            .context("access_storage_tagged")?
            .blocks_web3_dal()
            .get_sealed_miniblock_number()
            .await
            .context("get_sealed_miniblock_number()")
    }

    async fn current_subscribers(&self) -> Vec<V> {
        self.subscribers.read().await.values().cloned().collect()
    }
}

impl PubSubNotifier<typed::Sink<PubSubResult>> {
    pub async fn notify_blocks(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut last_block_number = self.sealed_miniblock_number().await?;
        let mut timer = interval(self.polling_interval);
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, pubsub_block_notifier is shutting down");
                break;
            }
            timer.tick().await;

            let db_latency = PUB_SUB_METRICS.db_poll_latency[&SubscriptionType::Blocks].start();
            let new_blocks = self.new_blocks(last_block_number).await?;
            db_latency.observe();

            if let Some(last_block) = new_blocks.last() {
                last_block_number = MiniblockNumber(last_block.number.unwrap().as_u32());

                let notify_latency =
                    PUB_SUB_METRICS.notify_subscribers_latency[&SubscriptionType::Blocks].start();
                for sink in self.current_subscribers().await {
                    for block in new_blocks.iter().cloned() {
                        if sink.notify(Ok(PubSubResult::Header(block))).is_err() {
                            // Subscriber disconnected.
                            break;
                        }
                        PUB_SUB_METRICS.notify[&SubscriptionType::Blocks].inc();
                    }
                }
                notify_latency.observe();
            }
        }
        Ok(())
    }

    async fn new_blocks(
        &self,
        last_block_number: MiniblockNumber,
    ) -> anyhow::Result<Vec<BlockHeader>> {
        self.connection_pool
            .access_storage_tagged("api")
            .await
            .context("access_storage_tagged")?
            .blocks_web3_dal()
            .get_block_headers_after(last_block_number)
            .await
            .with_context(|| format!("get_block_headers_after({last_block_number})"))
    }

    pub async fn notify_txs(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut last_time = chrono::Utc::now().naive_utc();
        let mut timer = interval(self.polling_interval);
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, pubsub_tx_notifier is shutting down");
                break;
            }
            timer.tick().await;

            let db_latency = PUB_SUB_METRICS.db_poll_latency[&SubscriptionType::Txs].start();
            let (new_txs, new_last_time) = self.new_txs(last_time).await?;
            db_latency.observe();

            if let Some(new_last_time) = new_last_time {
                last_time = new_last_time;
                let notify_latency =
                    PUB_SUB_METRICS.notify_subscribers_latency[&SubscriptionType::Txs].start();

                for sink in self.current_subscribers().await {
                    for tx_hash in new_txs.iter().cloned() {
                        if sink.notify(Ok(PubSubResult::TxHash(tx_hash))).is_err() {
                            // Subscriber disconnected.
                            break;
                        }
                        PUB_SUB_METRICS.notify[&SubscriptionType::Txs].inc();
                    }
                }
                notify_latency.observe();
            }
        }
        Ok(())
    }

    async fn new_txs(
        &self,
        last_time: chrono::NaiveDateTime,
    ) -> anyhow::Result<(Vec<H256>, Option<chrono::NaiveDateTime>)> {
        self.connection_pool
            .access_storage_tagged("api")
            .await
            .context("access_storage_tagged")?
            .transactions_web3_dal()
            .get_pending_txs_hashes_after(last_time, None)
            .await
            .context("get_pending_txs_hashes_after()")
    }
}

impl PubSubNotifier<(typed::Sink<PubSubResult>, PubSubFilter)> {
    pub async fn notify_logs(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut last_block_number = self.sealed_miniblock_number().await?;
        let mut timer = interval(self.polling_interval);
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, pubsub_logs_notifier is shutting down");
                break;
            }
            timer.tick().await;

            let db_latency = PUB_SUB_METRICS.db_poll_latency[&SubscriptionType::Logs].start();
            let new_logs = self.new_logs(last_block_number).await?;
            db_latency.observe();

            if let Some(last_log) = new_logs.last() {
                last_block_number = MiniblockNumber(last_log.block_number.unwrap().as_u32());
                let notify_latency =
                    PUB_SUB_METRICS.notify_subscribers_latency[&SubscriptionType::Logs].start();

                for (sink, filter) in self.current_subscribers().await {
                    for log in &new_logs {
                        if filter.matches(log) {
                            if sink.notify(Ok(PubSubResult::Log(log.clone()))).is_err() {
                                // Subscriber disconnected.
                                break;
                            }
                            PUB_SUB_METRICS.notify[&SubscriptionType::Logs].inc();
                        }
                    }
                }
                notify_latency.observe();
            }
        }
        Ok(())
    }

    async fn new_logs(&self, last_block_number: MiniblockNumber) -> anyhow::Result<Vec<Log>> {
        self.connection_pool
            .access_storage_tagged("api")
            .await
            .context("access_storage_tagged")?
            .events_web3_dal()
            .get_all_logs(last_block_number)
            .await
            .context("events_web3_dal().get_all_logs()")
    }
}
