use std::sync::Arc;

use zksync_core::api_server::{
    tree::TreeApiClient,
    tx_sender::{tx_sink::TxSink, TxSender},
};

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct TxSenderResource(pub TxSender);

impl Resource for TxSenderResource {
    fn name() -> String {
        "api/tx_sender".into()
    }
}

#[derive(Debug, Clone)]
pub struct TxSinkResource(pub Arc<dyn TxSink>);

impl Resource for TxSinkResource {
    fn name() -> String {
        "api/tx_sink".into()
    }
}

#[derive(Debug, Clone)]
pub struct TreeApiClientResource(pub Arc<dyn TreeApiClient>);

impl Resource for TreeApiClientResource {
    fn name() -> String {
        "api/tree_api_client".into()
    }
}
