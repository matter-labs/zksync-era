//! (Largely) backend-agnostic logic for dealing with Web3 subscriptions.

use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use tokio::{
    sync::{mpsc, watch, RwLock},
    task::JoinHandle,
    time::{interval, Duration},
};
use vise::GaugeGuard;
use zksync_dal::ConnectionPool;
use zksync_types::{MiniblockNumber, H128, H256};
use zksync_web3_decl::{
    jsonrpsee::{
        core::{server::SubscriptionMessage, SubscriptionResult},
        server::IdProvider,
        types::{error::ErrorCode, ErrorObject, SubscriptionId},
        PendingSubscriptionSink, SubscriptionSink,
    },
    namespaces::EthPubSubServer,
    types::{BlockHeader, Log, PubSubFilter, PubSubResult},
};

use super::{
    metrics::{SubscriptionType, PUB_SUB_METRICS},
    namespaces::eth::EVENT_TOPIC_NUMBER_LIMIT,
};

const SUBSCRIPTION_SINK_SEND_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy)]
pub struct EthSubscriptionIdProvider;

impl IdProvider for EthSubscriptionIdProvider {
    fn next_id(&self) -> SubscriptionId<'static> {
        let id = H128::random();
        format!("0x{}", hex::encode(id.0)).into()
    }
}

pub(super) type SubscriptionMap<T> = Arc<RwLock<HashMap<SubscriptionId<'static>, T>>>;

/// Events emitted by the subscription logic. Only used in WebSocket server tests so far.
#[derive(Debug)]
pub(super) enum PubSubEvent {
    Subscribed(SubscriptionType),
    NotifyIterationFinished(SubscriptionType),
}

/// Manager of notifications for a certain type of subscriptions.
#[derive(Debug)]
struct PubSubNotifier<V> {
    subscribers: SubscriptionMap<V>,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
}

struct MeteredSink {
    sink: SubscriptionSink,
    __guard: GaugeGuard,
}

impl AsRef<SubscriptionSink> for MeteredSink {
    fn as_ref(&self) -> &SubscriptionSink {
        &self.sink
    }
}

impl MeteredSink {
    fn new(sink: SubscriptionSink, sub_type: SubscriptionType) -> Self {
        Self {
            sink,
            __guard: PUB_SUB_METRICS.active_subscribers[&sub_type].inc_guard(1),
        }
    }
}

impl<V> PubSubNotifier<V> {
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

    fn emit_event(&self, event: PubSubEvent) {
        if let Some(sender) = &self.events_sender {
            sender.send(event).ok();
        }
    }
}

impl PubSubNotifier<MeteredSink> {
    async fn notify_blocks(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
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

                let subscribers = self.subscribers.read().await;
                let mut closed_subscriptions = vec![];

                for msink in subscribers.values() {
                    for block in new_blocks.iter().cloned() {
                        if msink
                            .sink
                            .send_timeout(
                                SubscriptionMessage::from_json(&PubSubResult::Header(block))
                                    .expect("PubSubResult always serializable to json;qed"),
                                SUBSCRIPTION_SINK_SEND_TIMEOUT,
                            )
                            .await
                            .is_err()
                        {
                            closed_subscriptions.push(msink.sink.subscription_id());
                            // Subscriber disconnected.
                            break;
                        }
                        PUB_SUB_METRICS.notify[&SubscriptionType::Blocks].inc();
                    }
                }
                notify_latency.observe();
                drop(subscribers);

                let mut subscribers_write = self.subscribers.write().await;
                for closed_sub in closed_subscriptions {
                    if subscribers_write.remove(&closed_sub).is_some() {
                        PUB_SUB_METRICS.active_subscribers[&SubscriptionType::Blocks].dec_by(1);
                    }
                }
            }
            self.emit_event(PubSubEvent::NotifyIterationFinished(
                SubscriptionType::Blocks,
            ));
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

    async fn notify_txs(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
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

                let subscribers = self.subscribers.read().await;
                let mut closed_subscriptions = vec![];

                for msink in subscribers.values() {
                    for tx_hash in new_txs.iter().cloned() {
                        if msink
                            .sink
                            .send_timeout(
                                SubscriptionMessage::from_json(&PubSubResult::TxHash(tx_hash))
                                    .expect("PubSubResult always serializable to json;qed"),
                                SUBSCRIPTION_SINK_SEND_TIMEOUT,
                            )
                            .await
                            .is_err()
                        {
                            closed_subscriptions.push(msink.sink.subscription_id());
                            // Subscriber disconnected.
                            break;
                        }
                        PUB_SUB_METRICS.notify[&SubscriptionType::Txs].inc();
                    }
                }
                notify_latency.observe();
                drop(subscribers);

                let mut subscribers_write = self.subscribers.write().await;
                for closed_sub in closed_subscriptions {
                    if subscribers_write.remove(&closed_sub).is_some() {
                        PUB_SUB_METRICS.active_subscribers[&SubscriptionType::Txs].dec_by(1);
                    }
                }
            }
            self.emit_event(PubSubEvent::NotifyIterationFinished(SubscriptionType::Txs));
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

impl PubSubNotifier<(MeteredSink, PubSubFilter)> {
    async fn notify_logs(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
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
                let subscribers = self.subscribers.read().await;
                let mut closed_subscriptions = vec![];

                for (msink, filter) in subscribers.values() {
                    for log in &new_logs {
                        if filter.matches(log) {
                            if msink
                                .sink
                                .send_timeout(
                                    SubscriptionMessage::from_json(&PubSubResult::Log(log.clone()))
                                        .expect("PubSubResult always serializable to json;qed"),
                                    SUBSCRIPTION_SINK_SEND_TIMEOUT,
                                )
                                .await
                                .is_err()
                            {
                                closed_subscriptions.push(msink.sink.subscription_id());
                                // Subscriber disconnected.
                                break;
                            }
                            PUB_SUB_METRICS.notify[&SubscriptionType::Logs].inc();
                        }
                    }
                }
                drop(subscribers);

                let mut subscribers_write = self.subscribers.write().await;
                for closed_sub in closed_subscriptions {
                    if subscribers_write.remove(&closed_sub).is_some() {
                        PUB_SUB_METRICS.active_subscribers[&SubscriptionType::Logs].dec_by(1);
                    }
                }
                notify_latency.observe();
            }
            self.emit_event(PubSubEvent::NotifyIterationFinished(SubscriptionType::Logs));
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

/// Subscription support for Web3 APIs.
pub(super) struct EthSubscribe {
    active_block_subs: SubscriptionMap<MeteredSink>,
    active_tx_subs: SubscriptionMap<MeteredSink>,
    active_log_subs: SubscriptionMap<(MeteredSink, PubSubFilter)>,
    events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
}

impl EthSubscribe {
    pub fn new() -> Self {
        Self {
            active_block_subs: SubscriptionMap::default(),
            active_tx_subs: SubscriptionMap::default(),
            active_log_subs: SubscriptionMap::default(),
            events_sender: None,
        }
    }

    pub fn set_events_sender(&mut self, sender: mpsc::UnboundedSender<PubSubEvent>) {
        self.events_sender = Some(sender);
    }

    async fn reject(sink: PendingSubscriptionSink) {
        sink.reject(ErrorObject::borrowed(
            ErrorCode::InvalidParams.code(),
            "Rejecting subscription - invalid parameters provided.",
            None,
        ))
        .await;
    }

    #[tracing::instrument(skip(self, pending_sink))]
    pub async fn sub(
        &self,
        pending_sink: PendingSubscriptionSink,
        sub_type: String,
        params: Option<PubSubFilter>,
    ) {
        let sub_type = match sub_type.as_str() {
            "newHeads" => {
                let mut block_subs = self.active_block_subs.write().await;
                let sink = pending_sink.accept().await.unwrap();

                block_subs.insert(
                    sink.subscription_id(),
                    MeteredSink::new(sink, SubscriptionType::Blocks),
                );
                Some(SubscriptionType::Blocks)
            }
            "newPendingTransactions" => {
                let mut tx_subs = self.active_tx_subs.write().await;
                let sink = pending_sink.accept().await.unwrap();
                tx_subs.insert(
                    sink.subscription_id(),
                    MeteredSink::new(sink, SubscriptionType::Txs),
                );
                Some(SubscriptionType::Txs)
            }
            "logs" => {
                let filter = params.unwrap_or_default();
                let topic_count = filter.topics.as_ref().map_or(0, Vec::len);
                if topic_count > EVENT_TOPIC_NUMBER_LIMIT {
                    Self::reject(pending_sink).await;
                    None
                } else {
                    let mut log_subs = self.active_log_subs.write().await;
                    let sink = pending_sink.accept().await.unwrap();
                    log_subs.insert(
                        sink.subscription_id(),
                        (MeteredSink::new(sink, SubscriptionType::Logs), filter),
                    );
                    Some(SubscriptionType::Logs)
                }
            }
            "syncing" => {
                let Ok(sink) = pending_sink.accept().await else {
                    return;
                };

                sink.send_timeout(
                    SubscriptionMessage::from_json(&PubSubResult::Syncing(false)).unwrap(),
                    SUBSCRIPTION_SINK_SEND_TIMEOUT,
                )
                .await
                .ok();
                None
            }
            _ => {
                Self::reject(pending_sink).await;
                None
            }
        };

        if let Some(sub_type) = sub_type {
            PUB_SUB_METRICS.active_subscribers[&sub_type].inc_by(1);
            if let Some(sender) = &self.events_sender {
                sender.send(PubSubEvent::Subscribed(sub_type)).ok();
            }
        }
    }

    /// Spawns notifier tasks. This should be called once per instance.
    pub fn spawn_notifiers(
        &self,
        connection_pool: ConnectionPool,
        polling_interval: Duration,
        stop_receiver: watch::Receiver<bool>,
    ) -> Vec<JoinHandle<anyhow::Result<()>>> {
        let mut notifier_tasks = Vec::with_capacity(3);
        let notifier = PubSubNotifier {
            subscribers: self.active_block_subs.clone(),
            connection_pool: connection_pool.clone(),
            polling_interval,
            events_sender: self.events_sender.clone(),
        };
        let notifier_task = tokio::spawn(notifier.notify_blocks(stop_receiver.clone()));
        notifier_tasks.push(notifier_task);

        let notifier = PubSubNotifier {
            subscribers: self.active_tx_subs.clone(),
            connection_pool: connection_pool.clone(),
            polling_interval,
            events_sender: self.events_sender.clone(),
        };
        let notifier_task = tokio::spawn(notifier.notify_txs(stop_receiver.clone()));
        notifier_tasks.push(notifier_task);

        let notifier = PubSubNotifier {
            subscribers: self.active_log_subs.clone(),
            connection_pool,
            polling_interval,
            events_sender: self.events_sender.clone(),
        };
        let notifier_task = tokio::spawn(notifier.notify_logs(stop_receiver));

        notifier_tasks.push(notifier_task);
        notifier_tasks
    }
}

#[async_trait::async_trait]
impl EthPubSubServer for EthSubscribe {
    async fn subscribe(
        &self,
        pending: PendingSubscriptionSink,
        sub_type: String,
        filter: Option<PubSubFilter>,
    ) -> SubscriptionResult {
        self.sub(pending, sub_type, filter).await;

        Ok(())
    }
}
