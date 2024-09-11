use std::sync::Arc;

use zksync_metadata_calculator::api_server::TreeApiClient;
use zksync_node_api_server::{
    tx_sender::{tx_sink::TxSink, TxSender},
    web3::mempool_cache::MempoolCache,
};

use crate::resource::Resource;

/// A resource that provides [`TxSender`] to the service.
#[derive(Debug, Clone)]
pub struct TxSenderResource(pub TxSender);

impl Resource for TxSenderResource {
    fn name() -> String {
        "api/tx_sender".into()
    }
}

impl From<TxSender> for TxSenderResource {
    fn from(sender: TxSender) -> Self {
        Self(sender)
    }
}

/// A resource that provides [`TxSink`] implementation to the service.
#[derive(Debug, Clone)]
pub struct TxSinkResource(pub Arc<dyn TxSink>);

impl Resource for TxSinkResource {
    fn name() -> String {
        "api/tx_sink".into()
    }
}

impl<T: TxSink> From<T> for TxSinkResource {
    fn from(sink: T) -> Self {
        Self(Arc::new(sink))
    }
}

/// A resource that provides [`TreeApiClient`] implementation to the service.
#[derive(Debug, Clone)]
pub struct TreeApiClientResource(pub Arc<dyn TreeApiClient>);

impl Resource for TreeApiClientResource {
    fn name() -> String {
        "api/tree_api_client".into()
    }
}

impl<T: TreeApiClient> From<Arc<T>> for TreeApiClientResource {
    fn from(client: Arc<T>) -> Self {
        Self(client)
    }
}

/// A resource that provides [`MempoolCache`] to the service.
#[derive(Debug, Clone)]
pub struct MempoolCacheResource(pub MempoolCache);

impl Resource for MempoolCacheResource {
    fn name() -> String {
        "api/mempool_cache".into()
    }
}

impl From<MempoolCache> for MempoolCacheResource {
    fn from(cache: MempoolCache) -> Self {
        Self(cache)
    }
}
