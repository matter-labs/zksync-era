use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use zksync_types::web3::types::H128;
use zksync_web3_decl::types::{PubSubFilter, PubSubResult};

use jsonrpc_core::error::{Error, ErrorCode};
use jsonrpc_pubsub::typed;
use jsonrpc_pubsub::SubscriptionId;

use super::eth::EVENT_TOPIC_NUMBER_LIMIT;

pub type SubscriptionMap<T> = Arc<RwLock<HashMap<SubscriptionId, T>>>;

#[derive(Debug, Clone, Default)]
pub struct EthSubscribe {
    pub active_block_subs: SubscriptionMap<typed::Sink<PubSubResult>>,
    pub active_tx_subs: SubscriptionMap<typed::Sink<PubSubResult>>,
    pub active_log_subs: SubscriptionMap<(typed::Sink<PubSubResult>, PubSubFilter)>,
}

impl EthSubscribe {
    fn assign_id(
        subscriber: typed::Subscriber<PubSubResult>,
    ) -> (typed::Sink<PubSubResult>, SubscriptionId) {
        let id = H128::random();
        let sub_id = SubscriptionId::String(format!("0x{}", hex::encode(id.0)));
        let sink = subscriber.assign_id(sub_id.clone()).unwrap();
        (sink, sub_id)
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
    pub fn sub(
        &self,
        subscriber: typed::Subscriber<PubSubResult>,
        sub_type: String,
        params: Option<serde_json::Value>,
    ) {
        let mut block_subs = self.active_block_subs.write().unwrap();
        let mut tx_subs = self.active_tx_subs.write().unwrap();
        let mut log_subs = self.active_log_subs.write().unwrap();
        match sub_type.as_str() {
            "newHeads" => {
                let (sink, id) = Self::assign_id(subscriber);
                block_subs.insert(id, sink);
            }
            "newPendingTransactions" => {
                let (sink, id) = Self::assign_id(subscriber);
                tx_subs.insert(id, sink);
            }
            "logs" => {
                let filter = params.map(serde_json::from_value).transpose();
                match filter {
                    Ok(filter) => {
                        let filter: PubSubFilter = filter.unwrap_or_default();
                        if filter
                            .topics
                            .as_ref()
                            .map(|topics| topics.len())
                            .unwrap_or(0)
                            > EVENT_TOPIC_NUMBER_LIMIT
                        {
                            Self::reject(subscriber);
                        } else {
                            let (sink, id) = Self::assign_id(subscriber);
                            log_subs.insert(id, (sink, filter));
                        }
                    }
                    Err(_) => Self::reject(subscriber),
                }
            }
            "syncing" => {
                let (sink, _) = Self::assign_id(subscriber);
                let _ = sink.notify(Ok(PubSubResult::Syncing(false)));
            }
            _ => Self::reject(subscriber),
        };

        metrics::gauge!("api.web3.pubsub.active_subscribers", block_subs.len() as f64, "subscription_type" => "blocks");
        metrics::gauge!("api.web3.pubsub.active_subscribers", tx_subs.len() as f64, "subscription_type" => "txs");
        metrics::gauge!("api.web3.pubsub.active_subscribers", log_subs.len() as f64, "subscription_type" => "logs");
    }

    #[tracing::instrument(skip(self))]
    pub fn unsub(&self, id: SubscriptionId) -> Result<bool, Error> {
        let removed = self
            .active_block_subs
            .write()
            .unwrap()
            .remove(&id)
            .or_else(|| self.active_tx_subs.write().unwrap().remove(&id))
            .or_else(|| {
                self.active_log_subs
                    .write()
                    .unwrap()
                    .remove(&id)
                    .map(|(sink, _)| sink)
            });
        if removed.is_some() {
            Ok(true)
        } else {
            Err(Error {
                code: ErrorCode::InvalidParams,
                message: "Invalid subscription.".into(),
                data: None,
            })
        }
    }
}
