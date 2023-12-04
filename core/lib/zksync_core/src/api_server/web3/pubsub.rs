//! (Largely) backend-agnostic logic for dealing with Web3 subscriptions.

use anyhow::Context as _;
use jsonrpc_core::error::{Error, ErrorCode};
use jsonrpc_pubsub::{typed, SubscriptionId};
use tokio::{
    sync::{mpsc, watch, RwLock},
    task::JoinHandle,
    time::{interval, Duration},
};
use zksync_server_dal::ServerStorageProcessor;

use std::{collections::HashMap, sync::Arc};

use zksync_db_connection::ConnectionPool;
use zksync_types::{MiniblockNumber, H128, H256};
use zksync_web3_decl::types::{BlockHeader, Log, PubSubFilter, PubSubResult};

use super::{
    metrics::{SubscriptionType, PUB_SUB_METRICS},
    namespaces::eth::EVENT_TOPIC_NUMBER_LIMIT,
};

pub(super) type SubscriptionMap<T> = Arc<RwLock<HashMap<SubscriptionId, T>>>;

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

impl<V: Clone> PubSubNotifier<V> {
    async fn sealed_miniblock_number(&self) -> anyhow::Result<MiniblockNumber> {
        self.connection_pool
            .access_storage_tagged::<ServerStorageProcessor>("api")
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

    fn emit_event(&self, event: PubSubEvent) {
        if let Some(sender) = &self.events_sender {
            sender.send(event).ok();
        }
    }
}

impl PubSubNotifier<typed::Sink<PubSubResult>> {
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
            .access_storage_tagged::<ServerStorageProcessor>("api")
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
            self.emit_event(PubSubEvent::NotifyIterationFinished(SubscriptionType::Txs));
        }
        Ok(())
    }

    async fn new_txs(
        &self,
        last_time: chrono::NaiveDateTime,
    ) -> anyhow::Result<(Vec<H256>, Option<chrono::NaiveDateTime>)> {
        self.connection_pool
            .access_storage_tagged::<ServerStorageProcessor>("api")
            .await
            .context("access_storage_tagged")?
            .transactions_web3_dal()
            .get_pending_txs_hashes_after(last_time, None)
            .await
            .context("get_pending_txs_hashes_after()")
    }
}

impl PubSubNotifier<(typed::Sink<PubSubResult>, PubSubFilter)> {
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
            self.emit_event(PubSubEvent::NotifyIterationFinished(SubscriptionType::Logs));
        }
        Ok(())
    }

    async fn new_logs(&self, last_block_number: MiniblockNumber) -> anyhow::Result<Vec<Log>> {
        self.connection_pool
            .access_storage_tagged::<ServerStorageProcessor>("api")
            .await
            .context("access_storage_tagged")?
            .events_web3_dal()
            .get_all_logs(last_block_number)
            .await
            .context("events_web3_dal().get_all_logs()")
    }
}

/// Subscription support for Web3 APIs.
#[derive(Debug, Clone)]
pub(super) struct EthSubscribe {
    // `jsonrpc` backend executes task subscription on a separate thread that has no tokio context.
    pub runtime_handle: tokio::runtime::Handle,
    active_block_subs: SubscriptionMap<typed::Sink<PubSubResult>>,
    active_tx_subs: SubscriptionMap<typed::Sink<PubSubResult>>,
    active_log_subs: SubscriptionMap<(typed::Sink<PubSubResult>, PubSubFilter)>,
    events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
}

impl EthSubscribe {
    pub fn new(runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            runtime_handle,
            active_block_subs: SubscriptionMap::default(),
            active_tx_subs: SubscriptionMap::default(),
            active_log_subs: SubscriptionMap::default(),
            events_sender: None,
        }
    }

    pub fn set_events_sender(&mut self, sender: mpsc::UnboundedSender<PubSubEvent>) {
        self.events_sender = Some(sender);
    }

    /// Assigns ID for the subscriber if the connection is open, returns error otherwise.
    fn assign_id(
        subscriber: typed::Subscriber<PubSubResult>,
    ) -> Result<(typed::Sink<PubSubResult>, SubscriptionId), ()> {
        let id = H128::random();
        let sub_id = SubscriptionId::String(format!("0x{}", hex::encode(id.0)));
        let sink = subscriber.assign_id(sub_id.clone())?;
        Ok((sink, sub_id))
    }

    fn reject(subscriber: typed::Subscriber<PubSubResult>) {
        subscriber
            .reject(Error {
                code: ErrorCode::InvalidParams,
                message: "Rejecting subscription - invalid parameters provided.".into(),
                data: None,
            })
            .unwrap();
    }

    #[tracing::instrument(skip(self, subscriber, params))]
    pub async fn sub(
        &self,
        subscriber: typed::Subscriber<PubSubResult>,
        sub_type: String,
        params: Option<serde_json::Value>,
    ) {
        let sub_type = match sub_type.as_str() {
            "newHeads" => {
                let mut block_subs = self.active_block_subs.write().await;
                let Ok((sink, id)) = Self::assign_id(subscriber) else {
                    return;
                };
                block_subs.insert(id, sink);
                Some(SubscriptionType::Blocks)
            }
            "newPendingTransactions" => {
                let mut tx_subs = self.active_tx_subs.write().await;
                let Ok((sink, id)) = Self::assign_id(subscriber) else {
                    return;
                };
                tx_subs.insert(id, sink);
                Some(SubscriptionType::Txs)
            }
            "logs" => {
                let filter = params.map(serde_json::from_value).transpose();
                match filter {
                    Ok(filter) => {
                        let filter: PubSubFilter = filter.unwrap_or_default();
                        let topic_count = filter.topics.as_ref().map_or(0, Vec::len);
                        if topic_count > EVENT_TOPIC_NUMBER_LIMIT {
                            Self::reject(subscriber);
                            None
                        } else {
                            let mut log_subs = self.active_log_subs.write().await;
                            let Ok((sink, id)) = Self::assign_id(subscriber) else {
                                return;
                            };
                            log_subs.insert(id, (sink, filter));
                            Some(SubscriptionType::Logs)
                        }
                    }
                    Err(_) => {
                        Self::reject(subscriber);
                        None
                    }
                }
            }
            "syncing" => {
                let Ok((sink, _id)) = Self::assign_id(subscriber) else {
                    return;
                };
                let _ = sink.notify(Ok(PubSubResult::Syncing(false)));
                None
            }
            _ => {
                Self::reject(subscriber);
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

    #[tracing::instrument(skip(self))]
    pub async fn unsub(&self, id: SubscriptionId) -> Result<bool, Error> {
        let removed = if self.active_block_subs.write().await.remove(&id).is_some() {
            Some(SubscriptionType::Blocks)
        } else if self.active_tx_subs.write().await.remove(&id).is_some() {
            Some(SubscriptionType::Txs)
        } else if self.active_log_subs.write().await.remove(&id).is_some() {
            Some(SubscriptionType::Logs)
        } else {
            None
        };
        if let Some(sub_type) = removed {
            PUB_SUB_METRICS.active_subscribers[&sub_type].dec_by(1);
            Ok(true)
        } else {
            Err(Error {
                code: ErrorCode::InvalidParams,
                message: "Invalid subscription.".into(),
                data: None,
            })
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
