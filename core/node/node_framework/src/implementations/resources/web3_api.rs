use zksync_core::api_server::tx_sender::TxSender;

use crate::resource::{Resource, ResourceId};

#[derive(Debug, Clone)]
pub struct TxSenderResource(pub TxSender);

impl Resource for TxSenderResource {
    fn resource_id() -> ResourceId {
        "api/tx_sender".into()
    }
}
