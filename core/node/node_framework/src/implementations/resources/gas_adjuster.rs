use std::sync::Arc;

use zksync_node_fee_model::l1_gas_price::GasAdjuster;

use crate::resource::Resource;

/// A resource that provides [`GasAdjuster`] to the service.
#[derive(Debug, Clone)]
pub struct GasAdjusterResource(pub Arc<GasAdjuster>);

impl Resource for GasAdjusterResource {
    fn name() -> String {
        "common/gas_adjuster".into()
    }
}

impl From<Arc<GasAdjuster>> for GasAdjusterResource {
    fn from(gas_adjuster: Arc<GasAdjuster>) -> Self {
        Self(gas_adjuster)
    }
}
