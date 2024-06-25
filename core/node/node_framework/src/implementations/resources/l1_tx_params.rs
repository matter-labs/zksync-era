use std::sync::Arc;

use zksync_node_fee_model::l1_gas_price::L1TxParamsProvider;

use crate::resource::Resource;

/// A resource that provides [`L1TxParamsProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct L1TxParamsResource(pub Arc<dyn L1TxParamsProvider>);

impl Resource for L1TxParamsResource {
    fn name() -> String {
        "common/l1_tx_params".into()
    }
}
