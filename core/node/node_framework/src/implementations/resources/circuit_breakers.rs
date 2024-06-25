use std::sync::Arc;

use zksync_circuit_breaker::CircuitBreakers;

use crate::resource::Resource;

/// A resource that provides [`CircuitBreakers`] to the service.
#[derive(Debug, Clone, Default)]
pub struct CircuitBreakersResource {
    pub breakers: Arc<CircuitBreakers>,
}

impl Resource for CircuitBreakersResource {
    fn name() -> String {
        "common/circuit_breakers".into()
    }
}
