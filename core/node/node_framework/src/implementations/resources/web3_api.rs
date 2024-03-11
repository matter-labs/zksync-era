use std::sync::Arc;

use zksync_core::api_server::{
    tree::TreeApiClient,
    tx_sender::{tx_sink::TxSink, TxSender},
};

use crate::resource::{Resource, ResourceId};

#[derive(Debug, Clone)]
pub struct TxSenderResource(pub TxSender);

impl Resource for TxSenderResource {
    fn resource_id() -> ResourceId {
        "api/tx_sender".into()
    }
}

#[derive(Debug, Clone)]
pub struct TxSinkResource(pub Arc<dyn TxSink>);

impl Resource for TxSinkResource {
    fn resource_id() -> ResourceId {
        "api/tx_sink".into()
    }
}

#[derive(Debug, Clone)]
pub struct TreeApiClientResource(pub Arc<dyn TreeApiClient>);

impl Resource for TreeApiClientResource {
    fn resource_id() -> ResourceId {
        "api/tree_api_client".into()
    }
}
