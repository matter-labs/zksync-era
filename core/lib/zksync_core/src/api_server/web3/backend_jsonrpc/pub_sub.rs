use std::sync::Arc;

use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::typed;
use jsonrpc_pubsub::{Session, SubscriptionId};

use zksync_web3_decl::types::PubSubResult;

use super::super::namespaces::EthSubscribe;
use super::batch_limiter_middleware::RateLimitMetadata;

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
    ) -> BoxFuture<Result<bool>>;
}

impl Web3PubSub for EthSubscribe {
    type Metadata = RateLimitMetadata<Arc<Session>>;

    fn subscribe(
        &self,
        _meta: Self::Metadata,
        subscriber: typed::Subscriber<PubSubResult>,
        sub_type: String,
        params: Option<serde_json::Value>,
    ) {
        let self_ = self.clone();
        // Fire and forget is OK here.
        self.runtime_handle
            .spawn(async move { self_.sub(subscriber, sub_type, params).await });
    }

    fn unsubscribe(
        &self,
        _meta: Option<Self::Metadata>,
        id: SubscriptionId,
    ) -> BoxFuture<Result<bool>> {
        let self_ = self.clone();
        Box::pin(async move { self_.unsub(id).await })
    }
}
