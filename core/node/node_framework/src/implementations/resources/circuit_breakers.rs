use std::sync::Arc;

use zksync_circuit_breaker::CircuitBreakers;

use crate::resource::Resource;

#[derive(Debug, Clone, Default)]
pub struct CircuitBreakersResource {
    pub breakers: Arc<CircuitBreakers>,
}

impl Resource for CircuitBreakersResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/circuit_breakers".into()
    }
}
