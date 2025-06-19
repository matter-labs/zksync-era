//! (Largely) backend-agnostic logic for dealing with Web3 subscriptions.

use std::time::Duration;

use chrono::NaiveDateTime;
use futures::FutureExt;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::JoinHandle,
};
use tracing::Instrument as _;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{L2BlockNumber, H128, H256};
use zksync_web3_decl::{
    jsonrpsee::{
        core::{server::SubscriptionMessage, SubscriptionResult},
        server::IdProvider,
        types::{error::ErrorCode, ErrorObject, SubscriptionId},
        PendingSubscriptionSink, SendTimeoutError, SubscriptionSink,
    },
    namespaces::EthPubSubServer,
    types::{BlockHeader, Log, PubSubFilter, PubSubResult},
};

use super::{
    metrics::{SubscriptionType, PUB_SUB_METRICS},
    namespaces::eth::EVENT_TOPIC_NUMBER_LIMIT,
};

const BROADCAST_CHANNEL_CAPACITY: usize = 1024;
const SUBSCRIPTION_SINK_SEND_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy)]
pub struct EthSubscriptionIdProvider;

impl IdProvider for EthSubscriptionIdProvider {
    fn next_id(&self) -> SubscriptionId<'static> {
        let id = H128::random();
        format!("0x{}", hex::encode(id.0)).into()
    }
}

/// Events emitted by the subscription logic. Only used in WebSocket server tests so far.
#[derive(Debug)]
pub enum PubSubEvent {
    Subscribed(SubscriptionType),
    NotifyIterationFinished(SubscriptionType),
    L2BlockAdvanced(SubscriptionType, L2BlockNumber),
}

/// Manager of notifications for a certain type of subscriptions.
#[derive(Debug)]
pub(crate) struct PubSubNotifier {
    ty: SubscriptionType,
    sender: broadcast::Sender<Vec<PubSubResult>>,
    connection_pool: ConnectionPool<Core>,
    polling_interval: Duration,
    events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
}

impl PubSubNotifier {
    pub(crate) fn subscription_type(&self) -> SubscriptionType {
        self.ty
    }

    // Notifier tasks are spawned independently of the main server task, so we need to wait for
    // Postgres to be non-empty separately.
    async fn get_starting_l2_block_number(
        &self,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<Option<L2BlockNumber>> {
        while !*stop_receiver.borrow_and_update() {
            let mut storage = self.connection_pool.connection_tagged("api").await?;
            if let Some(l2_block_number) = storage.blocks_dal().get_sealed_l2_block_number().await?
            {
                return Ok(Some(l2_block_number));
            }
            drop(storage);

            if tokio::time::timeout(self.polling_interval, stop_receiver.changed())
                .await
                .is_ok()
            {
                break;
            }
        }
        Ok(None) // we can only break from the loop if we've received a stop request
    }

    fn emit_event(&self, event: PubSubEvent) {
        if let Some(sender) = &self.events_sender {
            sender.send(event).ok();
        }
    }

    async fn notify_blocks(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let Some(mut last_block_number) = self
            .get_starting_l2_block_number(&mut stop_receiver)
            .await?
        else {
            tracing::info!("Stop request received, pubsub_block_notifier is shutting down");
            return Ok(());
        };

        let mut timer = tokio::time::interval(self.polling_interval);
        while !*stop_receiver.borrow() {
            tokio::select! {
                _ = stop_receiver.changed() => break,
                _ = timer.tick() => { /* continue processing */ }
            }

            let db_latency = PUB_SUB_METRICS[&SubscriptionType::Blocks]
                .db_poll_latency
                .start();
            let new_blocks = self.new_blocks(last_block_number).await?;
            db_latency.observe();

            if let Some(last_block) = new_blocks.last() {
                last_block_number = L2BlockNumber(last_block.number.unwrap().as_u32());
                let new_blocks = new_blocks.into_iter().map(PubSubResult::Header).collect();
                self.send_pub_sub_results(new_blocks, SubscriptionType::Blocks);
                self.emit_event(PubSubEvent::L2BlockAdvanced(
                    SubscriptionType::Blocks,
                    last_block_number,
                ));
            }
            self.emit_event(PubSubEvent::NotifyIterationFinished(
                SubscriptionType::Blocks,
            ));
        }

        tracing::info!("Stop request received, pubsub_block_notifier is shutting down");
        Ok(())
    }

    fn send_pub_sub_results(&self, results: Vec<PubSubResult>, sub_type: SubscriptionType) {
        // Errors only on 0 receivers, but we want to go on if we have 0 subscribers so ignore the error.
        self.sender.send(results).ok();
        PUB_SUB_METRICS[&sub_type]
            .broadcast_channel_len
            .set(self.sender.len());
    }

    async fn new_blocks(
        &self,
        last_block_number: L2BlockNumber,
    ) -> anyhow::Result<Vec<BlockHeader>> {
        self.connection_pool
            .connection_tagged("api")
            .await?
            .blocks_web3_dal()
            .get_block_headers_after(last_block_number)
            .await
            .map_err(Into::into)
    }

    async fn notify_txs(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut last_time = chrono::Utc::now().naive_utc();
        let mut timer = tokio::time::interval(self.polling_interval);
        while !*stop_receiver.borrow() {
            tokio::select! {
                _ = stop_receiver.changed() => break,
                _ = timer.tick() => { /* continue processing */ }
            }

            let db_latency = PUB_SUB_METRICS[&SubscriptionType::Txs]
                .db_poll_latency
                .start();
            let new_txs = self.new_txs(last_time).await?;
            db_latency.observe();

            if let Some((new_last_time, _)) = new_txs.last() {
                last_time = *new_last_time;
                let new_txs = new_txs
                    .into_iter()
                    .map(|(_, tx_hash)| PubSubResult::TxHash(tx_hash))
                    .collect();
                self.send_pub_sub_results(new_txs, SubscriptionType::Txs);
            }
            self.emit_event(PubSubEvent::NotifyIterationFinished(SubscriptionType::Txs));
        }

        tracing::info!("Stop request received, pubsub_tx_notifier is shutting down");
        Ok(())
    }

    async fn new_txs(
        &self,
        last_time: NaiveDateTime,
    ) -> anyhow::Result<Vec<(NaiveDateTime, H256)>> {
        self.connection_pool
            .connection_tagged("api")
            .await?
            .transactions_web3_dal()
            .get_pending_txs_hashes_after(last_time, None)
            .await
            .map_err(Into::into)
    }

    async fn notify_logs(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let Some(mut last_block_number) = self
            .get_starting_l2_block_number(&mut stop_receiver)
            .await?
        else {
            tracing::info!("Stop request received, pubsub_logs_notifier is shutting down");
            return Ok(());
        };

        let mut timer = tokio::time::interval(self.polling_interval);
        while !*stop_receiver.borrow() {
            tokio::select! {
                _ = stop_receiver.changed() => break,
                _ = timer.tick() => { /* continue processing */ }
            }

            let db_latency = PUB_SUB_METRICS[&SubscriptionType::Logs]
                .db_poll_latency
                .start();
            let new_logs = self.new_logs(last_block_number).await?;
            db_latency.observe();

            if let Some(last_log) = new_logs.last() {
                last_block_number = L2BlockNumber(last_log.block_number.unwrap().as_u32());
                let new_logs = new_logs.into_iter().map(PubSubResult::Log).collect();
                self.send_pub_sub_results(new_logs, SubscriptionType::Logs);
                self.emit_event(PubSubEvent::L2BlockAdvanced(
                    SubscriptionType::Logs,
                    last_block_number,
                ));
            }
            self.emit_event(PubSubEvent::NotifyIterationFinished(SubscriptionType::Logs));
        }
        tracing::info!("Stop request received, pubsub_logs_notifier is shutting down");
        Ok(())
    }

    async fn new_logs(&self, last_block_number: L2BlockNumber) -> anyhow::Result<Vec<Log>> {
        self.connection_pool
            .connection_tagged("api")
            .await?
            .events_web3_dal()
            .get_all_logs(last_block_number)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        match self.ty {
            SubscriptionType::Blocks => self.notify_blocks(stop_receiver).await,
            SubscriptionType::Txs => self.notify_txs(stop_receiver).await,
            SubscriptionType::Logs => self.notify_logs(stop_receiver).await,
        }
    }
}

/// Subscription support for Web3 APIs.
#[derive(Debug)]
pub(crate) struct EthSubscribe {
    polling_interval: Duration,
    blocks: broadcast::Sender<Vec<PubSubResult>>,
    transactions: broadcast::Sender<Vec<PubSubResult>>,
    logs: broadcast::Sender<Vec<PubSubResult>>,
    events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
}

impl EthSubscribe {
    const DEFAULT_POLLING_INTERVAL: Duration = Duration::from_millis(200);

    pub fn new(polling_interval: Option<Duration>) -> Self {
        let (blocks, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (transactions, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let (logs, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Self {
            polling_interval: polling_interval.unwrap_or(Self::DEFAULT_POLLING_INTERVAL),
            blocks,
            transactions,
            logs,
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

    async fn run_subscriber(
        sink: SubscriptionSink,
        subscription_type: SubscriptionType,
        mut receiver: broadcast::Receiver<Vec<PubSubResult>>,
        filter: Option<PubSubFilter>,
    ) {
        let metrics = &PUB_SUB_METRICS[&subscription_type];
        let _guard = metrics.active_subscribers.inc_guard(1);
        let lifetime_latency = metrics.subscriber_lifetime.start();
        let closed = sink.closed().fuse();
        tokio::pin!(closed);

        loop {
            tokio::select! {
                new_items_result = receiver.recv() => {
                    let new_items = match new_items_result {
                        Ok(items) => items,
                        Err(broadcast::error::RecvError::Closed) => {
                            // The broadcast channel has closed because the notifier task is shut down.
                            // This is fine; we should just stop this task.
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(message_count)) => {
                            metrics
                                .skipped_broadcast_messages
                                .observe(message_count);
                            break;
                        }
                    };

                    let handle_result = Self::handle_new_items(
                        &sink,
                        subscription_type,
                        new_items,
                        filter.as_ref()
                    )
                    .await;
                    if handle_result.is_err() {
                        metrics.subscriber_send_timeouts.inc();
                        break;
                    }
                }
                _ = &mut closed => {
                    break;
                }
            }
        }
        lifetime_latency.observe();
    }

    async fn handle_new_items(
        sink: &SubscriptionSink,
        subscription_type: SubscriptionType,
        new_items: Vec<PubSubResult>,
        filter: Option<&PubSubFilter>,
    ) -> Result<(), SendTimeoutError> {
        let metrics = &PUB_SUB_METRICS[&subscription_type];
        let notify_latency = metrics.notify_subscribers_latency.start();
        for item in new_items {
            if let PubSubResult::Log(log) = &item {
                if let Some(filter) = &filter {
                    if !filter.matches(log) {
                        continue;
                    }
                }
            }

            sink.send_timeout(
                SubscriptionMessage::from_json(&item)
                    .expect("PubSubResult always serializable to json;qed"),
                SUBSCRIPTION_SINK_SEND_TIMEOUT,
            )
            .await?;

            metrics.notify.inc();
        }

        notify_latency.observe();
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, pending_sink))]
    async fn sub(
        &self,
        pending_sink: PendingSubscriptionSink,
        sub_type: String,
        params: Option<PubSubFilter>,
    ) {
        let sub_type = match sub_type.as_str() {
            "newHeads" => {
                let Ok(sink) = pending_sink.accept().await else {
                    return;
                };
                let blocks_rx = self.blocks.subscribe();
                tokio::spawn(
                    Self::run_subscriber(sink, SubscriptionType::Blocks, blocks_rx, None)
                        .in_current_span(),
                );

                Some(SubscriptionType::Blocks)
            }
            "newPendingTransactions" => {
                let Ok(sink) = pending_sink.accept().await else {
                    return;
                };
                let transactions_rx = self.transactions.subscribe();
                tokio::spawn(
                    Self::run_subscriber(sink, SubscriptionType::Txs, transactions_rx, None)
                        .in_current_span(),
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
                    let Ok(sink) = pending_sink.accept().await else {
                        return;
                    };
                    let logs_rx = self.logs.subscribe();
                    tokio::spawn(
                        Self::run_subscriber(sink, SubscriptionType::Logs, logs_rx, Some(filter))
                            .in_current_span(),
                    );
                    Some(SubscriptionType::Logs)
                }
            }
            "syncing" => {
                let Ok(sink) = pending_sink.accept().await else {
                    return;
                };

                tokio::spawn(async move {
                    sink.send_timeout(
                        SubscriptionMessage::from_json(&PubSubResult::Syncing(false)).unwrap(),
                        SUBSCRIPTION_SINK_SEND_TIMEOUT,
                    )
                    .await
                });
                None
            }
            _ => {
                Self::reject(pending_sink).await;
                None
            }
        };

        if let Some(sub_type) = sub_type {
            if let Some(sender) = &self.events_sender {
                sender.send(PubSubEvent::Subscribed(sub_type)).ok();
            }
        }
    }

    pub(crate) fn create_notifier(
        &self,
        ty: SubscriptionType,
        connection_pool: ConnectionPool<Core>,
    ) -> PubSubNotifier {
        let sender = match ty {
            SubscriptionType::Blocks => self.blocks.clone(),
            SubscriptionType::Txs => self.transactions.clone(),
            SubscriptionType::Logs => self.logs.clone(),
        };

        PubSubNotifier {
            ty,
            sender,
            connection_pool,
            polling_interval: self.polling_interval,
            events_sender: self.events_sender.clone(),
        }
    }

    /// Test-only helper spawning all 3 notifier tasks.
    pub(crate) fn spawn_notifiers(
        &self,
        connection_pool: ConnectionPool<Core>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Vec<JoinHandle<anyhow::Result<()>>> {
        [
            SubscriptionType::Blocks,
            SubscriptionType::Txs,
            SubscriptionType::Logs,
        ]
        .into_iter()
        .map(|ty| {
            let notifier = self.create_notifier(ty, connection_pool.clone());
            tokio::spawn(notifier.run(stop_receiver.clone()))
        })
        .collect()
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
