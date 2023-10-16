use jsonrpc_core::error::{Error, ErrorCode};
use jsonrpc_pubsub::{typed, SubscriptionId};
use tokio::sync::RwLock;

use std::{collections::HashMap, sync::Arc};

use zksync_types::web3::types::H128;
use zksync_web3_decl::types::{PubSubFilter, PubSubResult};

use super::eth::EVENT_TOPIC_NUMBER_LIMIT;
use crate::api_server::web3::metrics::{SubscriptionType, PUB_SUB_METRICS};

pub type SubscriptionMap<T> = Arc<RwLock<HashMap<SubscriptionId, T>>>;

#[derive(Debug, Clone)]
pub struct EthSubscribe {
    // `jsonrpc` backend executes task subscription on a separate thread that has no tokio context.
    pub runtime_handle: tokio::runtime::Handle,
    pub active_block_subs: SubscriptionMap<typed::Sink<PubSubResult>>,
    pub active_tx_subs: SubscriptionMap<typed::Sink<PubSubResult>>,
    pub active_log_subs: SubscriptionMap<(typed::Sink<PubSubResult>, PubSubFilter)>,
}

impl EthSubscribe {
    pub fn new(runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            runtime_handle,
            active_block_subs: SubscriptionMap::default(),
            active_tx_subs: SubscriptionMap::default(),
            active_log_subs: SubscriptionMap::default(),
        }
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
                        if filter
                            .topics
                            .as_ref()
                            .map(|topics| topics.len())
                            .unwrap_or(0)
                            > EVENT_TOPIC_NUMBER_LIMIT
                        {
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
}
