use anyhow::Context as _;
use jsonrpc_pubsub::typed;
use tokio::sync::watch;
use tokio::time::{interval, Duration};

use zksync_dal::ConnectionPool;
use zksync_types::MiniblockNumber;
use zksync_web3_decl::types::{PubSubFilter, PubSubResult};

use super::{
    metrics::{SubscriptionType, PUB_SUB_METRICS},
    namespaces::SubscriptionMap,
};

pub async fn notify_blocks(
    subscribers: SubscriptionMap<typed::Sink<PubSubResult>>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut last_block_number = connection_pool
        .access_storage_tagged("api")
        .await
        .unwrap()
        .blocks_web3_dal()
        .get_sealed_miniblock_number()
        .await
        .context("get_sealed_miniblock_number()")?;
    let mut timer = interval(polling_interval);
    loop {
        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received, pubsub_block_notifier is shutting down");
            break;
        }

        timer.tick().await;

        let db_latency = PUB_SUB_METRICS.db_poll_latency[&SubscriptionType::Blocks].start();
        let new_blocks = connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .blocks_web3_dal()
            .get_block_headers_after(last_block_number)
            .await
            .with_context(|| format!("get_block_headers_after({last_block_number})"))?;
        db_latency.observe();

        if !new_blocks.is_empty() {
            last_block_number =
                MiniblockNumber(new_blocks.last().unwrap().number.unwrap().as_u32());

            let notify_latency =
                PUB_SUB_METRICS.notify_subscribers_latency[&SubscriptionType::Blocks].start();
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
                    PUB_SUB_METRICS.notify[&SubscriptionType::Blocks].inc();
                }
            }
            notify_latency.observe();
        }
    }
    Ok(())
}

pub async fn notify_txs(
    subscribers: SubscriptionMap<typed::Sink<PubSubResult>>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut last_time = chrono::Utc::now().naive_utc();
    let mut timer = interval(polling_interval);
    loop {
        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received, pubsub_tx_notifier is shutting down");
            break;
        }

        timer.tick().await;

        let db_latency = PUB_SUB_METRICS.db_poll_latency[&SubscriptionType::Txs].start();
        let (new_txs, new_last_time) = connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .transactions_web3_dal()
            .get_pending_txs_hashes_after(last_time, None)
            .await
            .context("get_pending_txs_hashes_after()")?;
        db_latency.observe();

        if let Some(new_last_time) = new_last_time {
            last_time = new_last_time;
            let notify_latency =
                PUB_SUB_METRICS.notify_subscribers_latency[&SubscriptionType::Txs].start();

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
                    PUB_SUB_METRICS.notify[&SubscriptionType::Txs].inc();
                }
            }
            notify_latency.observe();
        }
    }
    Ok(())
}

pub async fn notify_logs(
    subscribers: SubscriptionMap<(typed::Sink<PubSubResult>, PubSubFilter)>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut last_block_number = connection_pool
        .access_storage_tagged("api")
        .await
        .unwrap()
        .blocks_web3_dal()
        .get_sealed_miniblock_number()
        .await
        .context("get_sealed_miniblock_number()")?;
    let mut timer = interval(polling_interval);
    loop {
        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received, pubsub_logs_notifier is shutting down");
            break;
        }

        timer.tick().await;

        let db_latency = PUB_SUB_METRICS.db_poll_latency[&SubscriptionType::Logs].start();
        let new_logs = connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .events_web3_dal()
            .get_all_logs(last_block_number)
            .await
            .context("events_web3_dal().get_all_logs()")?;
        db_latency.observe();

        if !new_logs.is_empty() {
            last_block_number =
                MiniblockNumber(new_logs.last().unwrap().block_number.unwrap().as_u32());
            let notify_latency =
                PUB_SUB_METRICS.notify_subscribers_latency[&SubscriptionType::Logs].start();

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
                        PUB_SUB_METRICS.notify[&SubscriptionType::Logs].inc();
                    }
                }
            }
            notify_latency.observe();
        }
    }
    Ok(())
}
