use std::sync::Arc;

use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::typed;
use jsonrpc_pubsub::{Session, SubscriptionId};

use zksync_web3_decl::types::PubSubResult;

use super::super::namespaces::EthSubscribe;

#[rpc]
pub trait Web3PubSub {
    type Metadata;

    #[pubsub(subscription = "eth_subscription", subscribe, name = "eth_subscribe")]
    fn subscribe(
        &self,
        meta: Self::Metadata,
        subscriber: typed::Subscriber<PubSubResult>,
        sub_type: String,
        params: Option<serde_json::Value>,
    );

    #[pubsub(
        subscription = "eth_subscription",
        unsubscribe,
        name = "eth_unsubscribe"
    )]
    fn unsubscribe(
        &self,
        meta: Option<Self::Metadata>,
        subscription: SubscriptionId,
    ) -> Result<bool>;
}

impl Web3PubSub for EthSubscribe {
    type Metadata = Arc<Session>;

    fn subscribe(
        &self,
        _meta: Self::Metadata,
        subscriber: typed::Subscriber<PubSubResult>,
        sub_type: String,
        params: Option<serde_json::Value>,
    ) {
        self.sub(subscriber, sub_type, params);
    }

    fn unsubscribe(&self, _meta: Option<Self::Metadata>, id: SubscriptionId) -> Result<bool> {
        self.unsub(id)
    }
}
