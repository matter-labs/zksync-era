use std::sync::Arc;

use zksync_circuit_breaker::CircuitBreakerChecker;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct CircuitBreakerCheckerResource(pub Arc<CircuitBreakerChecker>);

impl Resource for CircuitBreakerCheckerResource {
    fn resource_id() -> crate::resource::ResourceId {
        "common/circuit_breaker_checker".into()
    }
}
