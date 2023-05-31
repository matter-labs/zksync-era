use jsonrpc_pubsub::typed;
use tokio::sync::watch;
use tokio::time::{interval, Duration, Instant};

use zksync_dal::ConnectionPool;
use zksync_types::MiniblockNumber;
use zksync_web3_decl::types::{PubSubFilter, PubSubResult};

use super::namespaces::eth_subscribe::SubscriptionMap;

pub async fn notify_blocks(
    subscribers: SubscriptionMap<typed::Sink<PubSubResult>>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) {
    let mut last_block_number = connection_pool
        .access_storage_blocking()
        .blocks_web3_dal()
        .get_sealed_miniblock_number()
        .unwrap();
    let mut timer = interval(polling_interval);
    loop {
        if *stop_receiver.borrow() {
            vlog::info!("Stop signal received, pubsub_block_notifier is shutting down");
            break;
        }

        timer.tick().await;

        let start = Instant::now();
        let new_blocks = connection_pool
            .access_storage_blocking()
            .blocks_web3_dal()
            .get_block_headers_after(last_block_number)
            .unwrap();
        metrics::histogram!("api.web3.pubsub.db_poll_latency", start.elapsed(), "subscription_type" => "blocks");
        if !new_blocks.is_empty() {
            last_block_number =
                MiniblockNumber(new_blocks.last().unwrap().number.unwrap().as_u32());
            let start = Instant::now();
            for sink in subscribers.read().unwrap().values() {
                for block in new_blocks.clone() {
                    let _ = sink.notify(Ok(PubSubResult::Header(block)));
                    metrics::counter!("api.web3.pubsub.notify", 1, "subscription_type" => "blocks");
                }
            }
            metrics::histogram!("api.web3.pubsub.notify_subscribers_latency", start.elapsed(), "subscription_type" => "blocks");
        }
    }
}

pub async fn notify_txs(
    subscribers: SubscriptionMap<typed::Sink<PubSubResult>>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) {
    let mut last_time = chrono::Utc::now().naive_utc();
    let mut timer = interval(polling_interval);
    loop {
        if *stop_receiver.borrow() {
            vlog::info!("Stop signal received, pubsub_tx_notifier is shutting down");
            break;
        }

        timer.tick().await;

        let start = Instant::now();
        let (new_txs, new_last_time) = connection_pool
            .access_storage_blocking()
            .transactions_web3_dal()
            .get_pending_txs_hashes_after(last_time, None)
            .unwrap();
        metrics::histogram!("api.web3.pubsub.db_poll_latency", start.elapsed(), "subscription_type" => "txs");
        if let Some(new_last_time) = new_last_time {
            last_time = new_last_time;
            let start = Instant::now();
            for sink in subscribers.read().unwrap().values() {
                for tx_hash in new_txs.clone() {
                    let _ = sink.notify(Ok(PubSubResult::TxHash(tx_hash)));
                    metrics::counter!("api.web3.pubsub.notify", 1, "subscription_type" => "txs");
                }
            }
            metrics::histogram!("api.web3.pubsub.notify_subscribers_latency", start.elapsed(), "subscription_type" => "txs");
        }
    }
}

pub async fn notify_logs(
    subscribers: SubscriptionMap<(typed::Sink<PubSubResult>, PubSubFilter)>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) {
    let mut last_block_number = connection_pool
        .access_storage_blocking()
        .blocks_web3_dal()
        .get_sealed_miniblock_number()
        .unwrap();
    let mut timer = interval(polling_interval);
    loop {
        if *stop_receiver.borrow() {
            vlog::info!("Stop signal received, pubsub_logs_notifier is shutting down");
            break;
        }

        timer.tick().await;

        let start = Instant::now();
        let new_logs = connection_pool
            .access_storage_blocking()
            .events_web3_dal()
            .get_all_logs(last_block_number)
            .unwrap();
        metrics::histogram!("api.web3.pubsub.db_poll_latency", start.elapsed(), "subscription_type" => "logs");
        if !new_logs.is_empty() {
            last_block_number =
                MiniblockNumber(new_logs.last().unwrap().block_number.unwrap().as_u32());
            let start = Instant::now();
            for (sink, filter) in subscribers.read().unwrap().values() {
                for log in new_logs.clone() {
                    if filter.matches(&log) {
                        let _ = sink.notify(Ok(PubSubResult::Log(log)));
                        metrics::counter!("api.web3.pubsub.notify", 1, "subscription_type" => "logs");
                    }
                }
            }
            metrics::histogram!("api.web3.pubsub.notify_subscribers_latency", start.elapsed(), "subscription_type" => "logs");
        }
    }
}
