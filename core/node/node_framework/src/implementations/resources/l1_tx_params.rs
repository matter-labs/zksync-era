use std::sync::Arc;

use zksync_node_fee_model::l1_gas_price::TxParamsProvider;

use crate::resource::Resource;

/// A resource that provides [`L1TxParamsProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct L1TxParamsResource(pub Arc<dyn TxParamsProvider>);

impl Resource for L1TxParamsResource {
    fn name() -> String {
        "common/tx_params".into()
    }
}

impl<T: TxParamsProvider> From<Arc<T>> for L1TxParamsResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
