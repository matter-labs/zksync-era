use std::sync::Arc;

use zksync_node_framework::resource::Resource;

use crate::{
    tx_sender::{tx_sink::TxSink, TxSender},
    web3::mempool_cache::MempoolCache,
};

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
