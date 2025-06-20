use zksync_node_framework::{resource, resource::Resource};

use crate::{
    tx_sender::{tx_sink::TxSink, TxSender},
    web3::mempool_cache::MempoolCache,
};

impl Resource for TxSender {
    fn name() -> String {
        "api/tx_sender".into()
    }
}

impl Resource<resource::Shared> for dyn TxSink {
    fn name() -> String {
        "api/tx_sink".into()
    }
}

impl Resource for MempoolCache {
    fn name() -> String {
        "api/mempool_cache".into()
    }
}
