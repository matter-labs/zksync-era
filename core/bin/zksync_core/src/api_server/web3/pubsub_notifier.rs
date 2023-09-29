use jsonrpc_pubsub::typed;
use tokio::sync::watch;
use tokio::time::{interval, Duration, Instant};

use zksync_dal::ConnectionPool;
use zksync_types::MiniblockNumber;
use zksync_web3_decl::types::{PubSubFilter, PubSubResult};

use super::namespaces::SubscriptionMap;

pub async fn notify_blocks(
    subscribers: SubscriptionMap<typed::Sink<PubSubResult>>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) {
    let mut last_block_number = connection_pool
        .access_storage_tagged("api")
        .await
        .blocks_web3_dal()
        .get_sealed_miniblock_number()
        .await
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
            .access_storage_tagged("api")
            .await
            .blocks_web3_dal()
            .get_block_headers_after(last_block_number)
            .await
            .unwrap();
        metrics::histogram!("api.web3.pubsub.db_poll_latency", start.elapsed(), "subscription_type" => "blocks");
        if !new_blocks.is_empty() {
            last_block_number =
                MiniblockNumber(new_blocks.last().unwrap().number.unwrap().as_u32());

            let start = Instant::now();
            let subscribers = subscribers
                .read()
                .await
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for sink in subscribers {
                for block in new_blocks.iter().cloned() {
                    if sink.notify(Ok(PubSubResult::Header(block))).is_err() {
                        // Subscriber disconnected.
                        break;
                    }
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
            .access_storage_tagged("api")
            .await
            .transactions_web3_dal()
            .get_pending_txs_hashes_after(last_time, None)
            .await
            .unwrap();
        metrics::histogram!("api.web3.pubsub.db_poll_latency", start.elapsed(), "subscription_type" => "txs");
        if let Some(new_last_time) = new_last_time {
            last_time = new_last_time;
            let start = Instant::now();

            let subscribers = subscribers
                .read()
                .await
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for sink in subscribers {
                for tx_hash in new_txs.iter().cloned() {
                    if sink.notify(Ok(PubSubResult::TxHash(tx_hash))).is_err() {
                        // Subscriber disconnected.
                        break;
                    }
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
        .access_storage_tagged("api")
        .await
        .blocks_web3_dal()
        .get_sealed_miniblock_number()
        .await
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
            .access_storage_tagged("api")
            .await
            .events_web3_dal()
            .get_all_logs(last_block_number)
            .await
            .unwrap();
        metrics::histogram!("api.web3.pubsub.db_poll_latency", start.elapsed(), "subscription_type" => "logs");
        if !new_logs.is_empty() {
            last_block_number =
                MiniblockNumber(new_logs.last().unwrap().block_number.unwrap().as_u32());
            let start = Instant::now();

            let subscribers = subscribers
                .read()
                .await
                .values()
                .cloned()
                .collect::<Vec<_>>();

            for (sink, filter) in subscribers {
                for log in new_logs.iter().cloned() {
                    if filter.matches(&log) {
                        if sink.notify(Ok(PubSubResult::Log(log))).is_err() {
                            // Subscriber disconnected.
                            break;
                        }
                        metrics::counter!("api.web3.pubsub.notify", 1, "subscription_type" => "logs");
                    }
                }
            }
            metrics::histogram!("api.web3.pubsub.notify_subscribers_latency", start.elapsed(), "subscription_type" => "logs");
        }
    }
}
