use std::sync::Arc;

use zksync_node_fee_model::l1_gas_price::TxParamsProvider;

use crate::resource::Resource;

/// A resource that provides [`TxParamsProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct TxParamsResource(pub Arc<dyn TxParamsProvider>);

impl Resource for TxParamsResource {
    fn name() -> String {
        "common/tx_params".into()
    }
}

impl<T: TxParamsProvider> From<Arc<T>> for TxParamsResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
