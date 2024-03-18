use std::sync::Arc;

use zksync_core::l1_gas_price::L1TxParamsProvider;

use crate::resource::{Resource, ResourceId};

/// Wrapper for the l1 tx params provider.
#[derive(Debug, Clone)]
pub struct L1TxParamsResource(pub Arc<dyn L1TxParamsProvider>);

impl Resource for L1TxParamsResource {
    fn resource_id() -> ResourceId {
        "common/l1_tx_params".into()
    }
}
